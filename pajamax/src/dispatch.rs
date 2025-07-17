//! Dispatch mode.
//!
//! See the module's document for details.

use std::net::TcpStream;
use std::sync::{mpsc, Arc, Mutex};

use crate::config::Config;
use crate::error::Error;
use crate::response_end::ResponseEnd;
use crate::status::{Code, Status};
use crate::ReplyEncode;
use crate::Response;

/// Send end of request channel for dispatch mode.
pub type RequestTx<Req> = mpsc::SyncSender<DispatchRequest<Req>>;

/// Receive end of request channel for dispatch mode.
pub type RequestRx<Req> = mpsc::Receiver<DispatchRequest<Req>>;

/// Send end of response channel for dispatch mode.
type ResponseTx = mpsc::SyncSender<DispatchResponse>;

/// Receive end of response channel for dispatch mode.
type ResponseRx = mpsc::Receiver<DispatchResponse>;

/// Dispatched request in dispatch mode.
pub struct DispatchRequest<Req> {
    pub stream_id: u32,
    pub req_data_len: usize,
    pub request: Req,
    pub resp_tx: ResponseTx,
}

/// Dispatched response in dispatch mode.
pub struct DispatchResponse {
    pub stream_id: u32,
    pub req_data_len: usize,
    pub response: Response<Box<dyn ReplyEncode>>,
}

use std::cell::RefCell;
thread_local! {
    static RESP_TX: RefCell<Option<ResponseTx>> = RefCell::new(None);
}

pub fn get_resp_tx(resp_end: &ResponseEnd) -> ResponseTx {
    RESP_TX.with_borrow_mut(|cell| match cell {
        None => {
            let c = resp_end.c.clone();
            let config = Config::new();
            let resp_tx = new_response_routine(c, &config);
            *cell = Some(resp_tx.clone());
            resp_tx
        }
        Some(resp_tx) => resp_tx.clone(),
    })
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

pub fn dispatch<Req>(
    req_tx: &RequestTx<Req>,
    request: Req,
    stream_id: u32,
    req_data_len: usize,
    resp_end: &mut ResponseEnd,
) -> Response<()> {
    let disp_req = DispatchRequest {
        request,
        stream_id,
        req_data_len,
        resp_tx: get_resp_tx(resp_end),
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

        resp_end.build2(resp.stream_id, resp.response, resp.req_data_len);
        resp_end.flush(false)?;
    }

    Err(Error::ChannelClosed)
}
