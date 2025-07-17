use std::cell::RefCell;
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

thread_local! {
    static RESP_TX: RefCell<ResponseTx> = panic!();
}

// create a backend thread with response-channels
pub fn new_response_routine(c: Arc<Mutex<TcpStream>>, config: &Config) {
    let resp_end = ResponseEnd::new(c, config);

    let (resp_tx, resp_rx) = mpsc::sync_channel(config.max_concurrent_streams);

    RESP_TX.set(resp_tx);

    std::thread::Builder::new()
        .name(String::from("pajamax-r")) // response routine
        .spawn(move || response_routine(resp_end, resp_rx))
        .unwrap();
}

// dispatch the request to req_tx
pub fn dispatch<Req>(
    req_tx: &RequestTx<Req>,
    request: Req,
    stream_id: u32,
    req_data_len: usize,
    resp_end: &mut ResponseEnd,
) -> Result<(), Error> {
    let disp_req = DispatchRequest {
        request,
        stream_id,
        req_data_len,
        resp_tx: RESP_TX.with_borrow(|tx| tx.clone()),
    };

    match req_tx.try_send(disp_req) {
        Ok(_) => Ok(()),
        Err(err) => {
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
            let response: Response<()> = Err(status);
            Ok(resp_end.build(stream_id, response, req_data_len)?)
        }
    }
}

// output thread
fn response_routine(mut resp_end: ResponseEnd, resp_rx: ResponseRx) -> Result<(), Error> {
    loop {
        let resp = match resp_rx.try_recv() {
            Ok(resp) => resp,
            Err(mpsc::TryRecvError::Disconnected) => {
                break Err(Error::ChannelClosed);
            }
            Err(mpsc::TryRecvError::Empty) => {
                resp_end.flush()?;
                resp_rx.recv()? // blocking mode
            }
        };

        resp_end.build_box(resp.stream_id, resp.response, resp.req_data_len)?;
    }
}
