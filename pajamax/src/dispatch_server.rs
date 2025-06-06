//! Dispatch mode.
//!
//! See the module's document for details.

use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::config::Config;
use crate::connection::ConnectionMode;
use crate::error::Error;
use crate::response_end::ResponseEnd;
use crate::status::{Code, Status};
use crate::Response;
use crate::{PajamaxService, RespEncode};

/// Send end of request channel for dispatch mode.
pub type RequestTx<Req, Reply> = mpsc::SyncSender<DispatchRequest<Req, Reply>>;

/// Receive end of request channel for dispatch mode.
pub type RequestRx<Req, Reply> = mpsc::Receiver<DispatchRequest<Req, Reply>>;

/// Send end of response channel for dispatch mode.
type ResponseTx<Reply> = mpsc::SyncSender<DispatchResponse<Reply>>;

/// Receive end of response channel for dispatch mode.
type ResponseRx<Reply> = mpsc::Receiver<DispatchResponse<Reply>>;

/// Dispatched request in dispatch mode.
pub struct DispatchRequest<Req, Reply> {
    stream_id: u32,
    req_data_len: usize,
    request: Req,
    resp_tx: ResponseTx<Reply>,
}

/// Dispatched response in dispatch mode.
struct DispatchResponse<Reply> {
    stream_id: u32,
    req_data_len: usize,
    response: Response<Reply>,
}

impl<Req, Reply> DispatchRequest<Req, Reply> {
    // handle the request
    // call its method and send it back to response channel
    pub fn handle<S>(self, ctx: &mut S)
    where
        S: PajamaxService<Request = Req, Reply = Reply>,
    {
        let Self {
            request,
            stream_id,
            req_data_len,
            resp_tx,
        } = self;

        let response = ctx.call(request);

        let resp = DispatchResponse {
            stream_id,
            req_data_len,
            response,
        };

        let _ = resp_tx.send(resp);
    }
}

/// `pajamax-build` crate implements this for dispatch mode.
pub trait PajamaxDispatchService: PajamaxService {
    fn dispatch_to(
        &self,
        request: &Self::Request,
    ) -> Option<&RequestTx<Self::Request, Self::Reply>>;
}

pub(crate) struct DispatchConnection<S: PajamaxDispatchService> {
    srv: S,
    resp_tx: ResponseTx<S::Reply>,
    counter: Arc<AtomicUsize>,
}

impl<S: PajamaxDispatchService> DispatchConnection<S> {
    pub fn new(srv: S, c: &TcpStream, counter: Arc<AtomicUsize>, config: &Config) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);

        let resp_end = ResponseEnd::new(&c, config);

        let (resp_tx, resp_rx) = mpsc::sync_channel(config.max_concurrent_streams);
        std::thread::Builder::new()
            .name(String::from("pajamax-dmo")) // dispatch-mode-output
            .spawn(move || response_routine(resp_end, resp_rx))
            .unwrap();

        Self {
            srv,
            resp_tx,
            counter,
        }
    }
}

impl<S: PajamaxDispatchService> Drop for DispatchConnection<S> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

// output thread
fn response_routine<Reply: RespEncode + Send + Sync + 'static>(
    mut resp_end: ResponseEnd,
    resp_rx: ResponseRx<Reply>,
) -> Result<(), Error> {
    loop {
        let resp = match resp_rx.try_recv() {
            Ok(resp) => resp,
            Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {
                resp_end.flush(true)?;
                resp_rx.recv()?
            }
        };

        resp_end.build(resp.stream_id, resp.response, resp.req_data_len);
        resp_end.flush(false)?;
    }

    Err(Error::ChannelClosed)
}

impl<S: PajamaxDispatchService> ConnectionMode for DispatchConnection<S> {
    type Service = S;

    fn handle_call(
        &mut self,
        request: S::Request,
        stream_id: u32,
        req_data_len: usize,
    ) -> Result<(), std::io::Error> {
        match self.srv.dispatch_to(&request) {
            Some(req_tx) => {
                let disp_req = DispatchRequest {
                    request,
                    stream_id,
                    req_data_len,
                    resp_tx: self.resp_tx.clone(),
                };

                // dispatch the request by channel
                if let Err(err) = req_tx.try_send(disp_req) {
                    // if dispatch fails,
                    let status = match err {
                        mpsc::TrySendError::Full(_) => Status {
                            code: Code::Unavailable,
                            message: String::from("dispatch channel is full"),
                        },
                        mpsc::TrySendError::Disconnected(_) => Status {
                            code: Code::Internal,
                            message: String::from("dispatch channel is closed"),
                        },
                    };

                    // send error response
                    let disp_resp = DispatchResponse {
                        response: Err(status),
                        stream_id,
                        req_data_len,
                    };
                    let _ = self.resp_tx.send(disp_resp);
                }
            }
            None => {
                // handle the request directly
                let response = self.srv.call(request);

                let disp_resp = DispatchResponse {
                    response,
                    stream_id,
                    req_data_len,
                };
                let _ = self.resp_tx.send(disp_resp);
            }
        }
        Ok(())
    }
}
