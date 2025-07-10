//! Dispatch mode.
//!
//! See the module's document for details.

use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};

use crate::config::Config;
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

pub(crate) struct DispatchCtx<S: PajamaxService> {
    c: Arc<Mutex<TcpStream>>,
    config: Config,
    resp_tx: Option<ResponseTx<S::Reply>>,
}

impl<S: PajamaxService> DispatchCtx<S> {
    pub fn new(c: Arc<Mutex<TcpStream>>, config: Config) -> Self {
        Self {
            c,
            config,
            resp_tx: None,
        }
    }

    fn new_resp_tx(&mut self) -> ResponseTx<S::Reply> {
        if self.resp_tx.is_none() {
            let resp_end = ResponseEnd::new(self.c.clone(), &self.config);

            let (resp_tx, resp_rx) = mpsc::sync_channel(self.config.max_concurrent_streams);

            std::thread::Builder::new()
                .name(String::from("pajamax-r")) // response routing
                .spawn(move || response_routine(resp_end, resp_rx))
                .unwrap();

            self.resp_tx = Some(resp_tx);
        }

        self.resp_tx.as_ref().unwrap().clone()
    }

    pub fn dispatch(
        &mut self,
        req_tx: &RequestTx<S::Request, S::Reply>,
        request: S::Request,
        stream_id: u32,
        req_data_len: usize,
    ) -> Response<()> {
        let disp_req = DispatchRequest {
            request,
            stream_id,
            req_data_len,
            resp_tx: self.new_resp_tx(),
        };

        req_tx.try_send(disp_req).map_err(|err| match err {
            mpsc::TrySendError::Full(_) => Status {
                code: Code::Unavailable,
                message: String::from("dispatch channel is full"),
            },
            mpsc::TrySendError::Disconnected(_) => Status {
                code: Code::Internal,
                message: String::from("dispatch channel is closed"),
            },
        })
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
