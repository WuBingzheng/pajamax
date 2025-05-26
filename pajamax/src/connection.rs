use std::collections::HashMap;
use std::io::Read;

use mio::net::TcpStream; // TODO remove mio here

use crate::config::Config;
use crate::error::Error;
use crate::hpack_decoder::Decoder;
use crate::http2::*;
use crate::{PajamaxService, ParseFn};

// implemented by local mode and dispatch mode
pub trait ConnectionMode {
    type Service: PajamaxService;

    fn handle_call(
        &mut self,
        request: <Self::Service as PajamaxService>::Request,
        stream_id: u32,
        req_data_len: usize,
    ) -> Result<(), std::io::Error>;

    fn defer_flush(&mut self) -> Result<(), std::io::Error> {
        Ok(())
    }
}

pub struct Connection<S: ConnectionMode> {
    c: TcpStream,
    srv_conn: S,
    input: Vec<u8>,
    streams: HashMap<u32, ParseFn<<S::Service as PajamaxService>::Request>>,
    hpack_decoder: Decoder<S::Service>,
    has_handshaked: bool,
    last_end: usize,
}

impl<S: ConnectionMode> Connection<S> {
    pub fn new(srv_conn: S, c: TcpStream, config: &Config) -> Self {
        let mut input = Vec::new();
        input.resize(config.max_frame_size, 0);
        Self {
            c,
            srv_conn,
            input,
            streams: HashMap::new(),
            hpack_decoder: Decoder::new(),
            has_handshaked: false,
            last_end: 0,
        }
    }

    pub fn handle(&mut self) -> Result<usize, Error>
    where
        S: ConnectionMode,
    {
        if !self.has_handshaked {
            handshake(&mut self.c, &Config::new())?;
            self.has_handshaked = true;
        }

        loop {
            let len = self.c.read(&mut self.input[self.last_end..])?;
            if len == 0 {
                // connection closed
                return Ok(0);
            }
            let end = self.last_end + len;

            let mut pos = 0;
            while let Some(frame) = Frame::parse(&self.input[pos..end]) {
                pos += Frame::HEAD_SIZE + frame.len; // for next loop

                match frame.kind {
                    FrameKind::Data => {
                        let req_buf = frame.process_data()?;

                        // grpc-level-protocal
                        if req_buf.len() == 0 {
                            continue;
                        }
                        if req_buf.len() < 5 {
                            return Err(Error::InvalidHttp2("DATA frame too short for grpc"));
                        }
                        let req_buf = &req_buf[5..];

                        // find the request-parse-fn
                        let stream_id = frame.stream_id;
                        let Some(parse_fn) = self.streams.remove(&stream_id) else {
                            return Err(Error::InvalidHttp2("DATA frame without HEADERS"));
                        };

                        let request = (parse_fn)(req_buf)?;

                        // call the method!
                        self.srv_conn.handle_call(request, stream_id, frame.len)?;
                    }
                    FrameKind::Headers => {
                        let headers_buf = frame.process_headers()?;

                        let parse_fn = self.hpack_decoder.find_path(headers_buf)?;

                        if self.streams.insert(frame.stream_id, parse_fn).is_some() {
                            return Err(Error::InvalidHttp2("duplicated HEADERS frame"));
                        }
                    }
                    _ => (),
                }
            }

            self.srv_conn.defer_flush()?;

            // for next loop
            if pos == 0 {
                return Err(Error::InvalidHttp2("too long frame"));
            }
            if pos < end {
                self.input.copy_within(pos..end, 0);
                self.last_end = end - pos;
            } else {
                self.last_end = 0;
            }
        }
    }
}
