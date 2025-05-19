use std::net::TcpStream;

use crate::connection::ConnectionMode;
use crate::response_end::ResponseEnd;
use crate::PajamaxService;

pub struct LocalConnection<S: PajamaxService> {
    srv: S,
    resp_end: ResponseEnd,
}

impl<S: PajamaxService> LocalConnection<S> {
    pub fn new(srv: S, c: &TcpStream) -> Self {
        Self {
            srv,
            resp_end: ResponseEnd::new(c),
        }
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

        self.resp_end.flush(15000)
    }

    fn defer_flush(&mut self) -> Result<(), std::io::Error> {
        self.resp_end.flush(0)
    }
}
