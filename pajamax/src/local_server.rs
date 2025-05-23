use std::net::TcpStream;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use crate::config::Config;
use crate::connection::ConnectionMode;
use crate::response_end::ResponseEnd;
use crate::PajamaxService;

pub struct LocalConnection<S: PajamaxService> {
    srv: S,
    resp_end: ResponseEnd,
    counter: Arc<AtomicUsize>,
}

impl<S: PajamaxService> LocalConnection<S> {
    pub fn new(srv: S, c: &TcpStream, counter: Arc<AtomicUsize>, config: &Config) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self {
            srv,
            resp_end: ResponseEnd::new(c, config),
            counter,
        }
    }
}

impl<S: PajamaxService> Drop for LocalConnection<S> {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

impl<S: PajamaxService> ConnectionMode for LocalConnection<S> {
    type Service = S;

    fn handle_call(
        &mut self,
        request: S::Request,
        stream_id: u32,
        req_data_len: usize,
    ) -> Result<(), std::io::Error> {
        let response = self.srv.call(request);

        self.resp_end.build(stream_id, response, req_data_len);

        self.resp_end.flush(false)
    }

    fn defer_flush(&mut self) -> Result<(), std::io::Error> {
        self.resp_end.flush(true)
    }
}
