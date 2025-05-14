use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;

use log::*;

use crate::hpack_decoder::Decoder;
use crate::hpack_encoder::Encoder;
use crate::http2::*;
use crate::{AtiourService, ParseFn};

pub(crate) enum ParseError {
    InvalidHttp2(String),
    InvalidHpack(String),
    InvalidHuffman(String),
    UnknownMethod(String),
    NoPathSet,
}

pub struct Connection<S: AtiourService> {
    c: TcpStream,
    srv: S,

    streams: HashMap<u32, ParseFn<S::Request>>,
    hpack_decoder: Decoder<S::Request>,
    hpack_encoder: Encoder,
    req_data_len: usize, // for WINDOW_UPDATE
}

impl<S: AtiourService> Connection<S> {
    pub fn new(c: TcpStream, srv: S) -> Self {
        Self {
            c,
            srv,

            streams: HashMap::new(),
            hpack_decoder: Decoder::new(),
            hpack_encoder: Encoder::new(),
            req_data_len: 0,
        }
    }

    pub fn handle(mut self) {
        if !handshake(&mut self.c) {
            return;
        }

        let mut input = Vec::new();
        input.resize(16 * 1024, 0);

        let mut output = Vec::with_capacity(16 * 1024);

        let mut last_end = 0;
        while let Ok(len) = self.c.read(&mut input[last_end..]) {
            if len == 0 {
                trace!("connection closed");
                break;
            }
            let end = last_end + len;

            let mut pos = 0;
            while let Some(frame) = Frame::parse(&input[pos..end]) {
                pos += Frame::HEAD_SIZE + frame.len; // for next loop
                self.handle_frame(&frame, &mut output);

                if output.len() > 15 * 1024 {
                    self.flush_response(&mut output);
                }
            }

            // flush response
            if !output.is_empty() {
                self.flush_response(&mut output);
            }

            // for next loop
            if pos == 0 {
                warn!("too long frame, we current support 16K by now.");
                return;
            }
            if pos < end {
                trace!("not complete: {pos} {end}");
                input.copy_within(pos..end, 0);
                last_end = end - pos;
            } else {
                last_end = 0;
            }
        }
    }

    fn handle_frame(&mut self, frame: &Frame, output: &mut Vec<u8>) {
        match frame.kind {
            FrameKind::Data => {
                self.req_data_len += frame.len;

                let Some(req_buf) = frame.process_data() else {
                    return; // empty DATA with END_STREAM flag
                            // XXX continue
                };

                // grpc-level-protocal
                if req_buf.len() == 0 {
                    return; // continue
                }
                if req_buf.len() < 5 {
                    warn!("DATA frame invalid grpc-protocal");
                    return;
                }
                let req_buf = &req_buf[5..];

                // find the request-parse-fn
                let stream_id = frame.stream_id;
                let Some(parse_fn) = self.streams.remove(&stream_id) else {
                    warn!("DATA frame without HEADERS");
                    return;
                };

                let request = match (parse_fn)(req_buf) {
                    Ok(request) => request,
                    Err(err) => {
                        warn!("fail in parse request: {:?}", err);
                        return;
                    }
                };

                // call the handler!
                match self.srv.call(request) {
                    Ok(reply) => {
                        build_response(stream_id, reply, &mut self.hpack_encoder, output);
                    }
                    Err(status) => {
                        build_status(stream_id, status, &mut self.hpack_encoder, output);
                    }
                }
            }
            FrameKind::Headers => {
                let Some(headers_buf) = frame.process_headers() else {
                    return;
                };

                let parse_fn = match self
                    .hpack_decoder
                    .find_path(headers_buf, S::request_parse_fn_by_path) // TODO use S in Decoder
                {
                    Ok(parse_fn) => parse_fn,
                    Err(err) => {
                        warn!("fain in find path: {:?}", err);
                        return;
                    }
                };

                if self.streams.insert(frame.stream_id, parse_fn).is_some() {
                    info!("duplicate HEADERS frame");
                    return;
                }
            }
            k => trace!("omit other frames: {:?}", k),
        }
    }

    fn flush_response(&mut self, output: &mut Vec<u8>) {
        build_window_update(self.req_data_len, output);
        if let Err(err) = self.c.write_all(output) {
            info!("connection send fail: {:?}", err);
            return;
        }
        output.clear();
        self.req_data_len = 0;
    }
}
