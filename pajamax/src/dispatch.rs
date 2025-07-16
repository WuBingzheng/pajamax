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
//use crate::{PajamaxService, RespEncode};

/// Send end of request channel for dispatch mode.
pub type RequestTx<Req, Reply> = mpsc::SyncSender<DispatchRequest<Req, Reply>>;

/// Receive end of request channel for dispatch mode.
pub type RequestRx<Req, Reply> = mpsc::Receiver<DispatchRequest<Req, Reply>>;

/// Send end of response channel for dispatch mode.
type ResponseTx = mpsc::SyncSender<DispatchResponse>;

/// Receive end of response channel for dispatch mode.
type ResponseRx = mpsc::Receiver<DispatchResponse>;

pub enum DispatchResult<Req, Reply> {
    Dispatch(RequestTx<Req>),
    Local(Response<Reply>),
}

/// Dispatched request in dispatch mode.
pub struct DispatchRequest<Req> {
    stream_id: u32,
    req_data_len: usize,
    request: Req,
    resp_tx: ResponseTx,
}

/// Dispatched response in dispatch mode.
struct DispatchResponse {
    stream_id: u32,
    req_data_len: usize,
    response: Box<dyn prost::Message>,
}

impl<Req> DispatchRequest<Req> {
    // handle the request
    // call its method and send it back to response channel
    pub fn handle<S>(self, ctx: &mut S)
    where
        S: PajamaxDispatchShard,
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

pub fn new_response_routine(c: Arc<Mutex<TcpStream>>, config: &Config) -> ResponseTx {
    let resp_end = ResponseEnd::new(c, config);

    let (resp_tx, resp_rx) = mpsc::sync_channel(config.max_concurrent_streams);

    std::thread::Builder::new()
        .name(String::from("pajamax-r")) // response routine
        .spawn(move || response_routine(resp_end, resp_rx))
        .unwrap();

    resp_tx
}

pub fn dispatch<Req, Reply>(
    req_tx: &RequestTx<Req, Reply>,
    request: Req,
    stream_id: u32,
    req_data_len: usize,
    resp_tx: ResponseTx,
) -> Response<()> {
    let disp_req = DispatchRequest {
        request,
        stream_id,
        req_data_len,
        resp_tx,
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

// output thread
fn response_routine(mut resp_end: ResponseEnd, resp_rx: ResponseRx) -> Result<(), Error> {
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
