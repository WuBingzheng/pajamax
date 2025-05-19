use std::io::Write;
use std::net::TcpStream;

use crate::hpack_encoder::Encoder;
use crate::http2;
use crate::Response;

pub struct ResponseEnd {
    c: TcpStream,
    req_data_len: usize,
    hpack_encoder: Encoder,
    output: Vec<u8>,
}

impl ResponseEnd {
    pub fn new(c: &TcpStream) -> Self {
        Self {
            c: c.try_clone().unwrap(),
            req_data_len: 0,
            hpack_encoder: Encoder::new(),
            output: Vec::with_capacity(16 * 1024),
        }
    }

    pub fn build<Reply: http2::RespEncode>(
        &mut self,
        stream_id: u32,
        response: Response<Reply>,
        req_data_len: usize,
    ) {
        self.req_data_len += req_data_len;
        match response {
            Ok(reply) => {
                http2::build_response(stream_id, reply, &mut self.hpack_encoder, &mut self.output);
            }
            Err(status) => {
                http2::build_status(stream_id, status, &mut self.hpack_encoder, &mut self.output);
            }
        }
    }

    pub fn flush(&mut self, min_len: usize) -> Result<(), std::io::Error> {
        if self.output.len() <= min_len {
            return Ok(());
        }

        http2::build_window_update(self.req_data_len, &mut self.output);

        self.c.write_all(&self.output)?;

        self.output.clear();
        self.req_data_len = 0;
        Ok(())
    }
}
