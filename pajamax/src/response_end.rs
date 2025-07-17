use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Mutex};

use crate::config::Config;
use crate::hpack_encoder::Encoder;
use crate::http2;
use crate::Response;

pub struct ResponseEnd {
    pub c: Arc<Mutex<TcpStream>>, // TODO pub
    req_count: usize,
    req_data_len: usize,
    hpack_encoder: Encoder,
    output: Vec<u8>,

    max_flush_requests: usize,
    max_flush_size: usize,
}

impl ResponseEnd {
    pub fn new(c: Arc<Mutex<TcpStream>>, config: &Config) -> Self {
        Self {
            c,
            req_count: 0,
            req_data_len: 0,
            hpack_encoder: Encoder::new(),
            output: Vec::with_capacity(config.max_flush_size),

            max_flush_requests: config.max_flush_requests,
            max_flush_size: config.max_flush_size,
        }
    }

    // build response to output buffer
    pub fn build<Reply>(
        &mut self,
        stream_id: u32,
        response: Response<Reply>,
        req_data_len: usize,
    ) -> Result<(), std::io::Error>
    where
        Reply: prost::Message,
    {
        match response {
            Ok(reply) => {
                http2::build_response(
                    stream_id,
                    |output| reply.encode(output).unwrap(),
                    &mut self.hpack_encoder,
                    &mut self.output,
                );
            }
            Err(status) => {
                http2::build_status(stream_id, status, &mut self.hpack_encoder, &mut self.output);
            }
        }

        self.update(req_data_len)
    }

    // build response to output buffer
    pub fn build_box(
        &mut self,
        stream_id: u32,
        response: Response<Box<dyn crate::http2::ReplyEncode>>,
        req_data_len: usize,
    ) -> Result<(), std::io::Error> {
        match response {
            Ok(reply) => {
                http2::build_response(
                    stream_id,
                    |output| reply.encode(output).unwrap(),
                    &mut self.hpack_encoder,
                    &mut self.output,
                );
            }
            Err(status) => {
                http2::build_status(stream_id, status, &mut self.hpack_encoder, &mut self.output);
            }
        }

        self.update(req_data_len)
    }

    fn update(&mut self, req_data_len: usize) -> Result<(), std::io::Error> {
        self.req_count += 1;
        self.req_data_len += req_data_len;

        if self.req_count >= self.max_flush_requests || self.output.len() >= self.max_flush_size {
            self.flush()
        } else {
            Ok(())
        }
    }

    // flush the output buffer
    pub fn flush(&mut self) -> Result<(), std::io::Error> {
        if self.output.len() == 0 {
            return Ok(());
        }

        http2::build_window_update(self.req_data_len, &mut self.output);

        self.c.lock().unwrap().write_all(&self.output)?;

        self.output.clear();
        self.req_count = 0;
        self.req_data_len = 0;
        Ok(())
    }
}
