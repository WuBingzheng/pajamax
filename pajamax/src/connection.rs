use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;

use crate::hpack_decoder::Decoder;
use crate::hpack_encoder::Encoder;
use crate::http2::*;
use crate::{PajamaxService, ParseFn};

#[allow(dead_code)]
#[derive(Debug)]
pub enum ParseError {
    InvalidHttp2(&'static str),
    InvalidHpack(&'static str),
    InvalidHuffman,
    InvalidProtobuf(prost::DecodeError),
    IoFail(std::io::Error),
    UnknownMethod(String),
    NoPathSet,
}

impl From<std::io::Error> for ParseError {
    fn from(io: std::io::Error) -> Self {
        Self::IoFail(io)
    }
}

impl From<prost::DecodeError> for ParseError {
    fn from(de: prost::DecodeError) -> Self {
        Self::InvalidProtobuf(de)
    }
}

pub struct Connection<S: PajamaxService> {
    c: TcpStream,
    srv: S,

    streams: HashMap<u32, ParseFn<S::Request>>,
    hpack_decoder: Decoder<S>,
    hpack_encoder: Encoder,
    req_data_len: usize, // for WINDOW_UPDATE
}

impl<S: PajamaxService> Connection<S> {
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

    pub fn handle(mut self) -> Result<(), ParseError> {
        handshake(&mut self.c)?;

        let mut input = Vec::new();
        input.resize(16 * 1024, 0);

        let mut output = Vec::with_capacity(16 * 1024);

        let mut last_end = 0;
        while let Ok(len) = self.c.read(&mut input[last_end..]) {
            if len == 0 {
                // connection closed
                break;
            }
            let end = last_end + len;

            let mut pos = 0;
            while let Some(frame) = Frame::parse(&input[pos..end]) {
                pos += Frame::HEAD_SIZE + frame.len; // for next loop
                self.handle_frame(&frame, &mut output)?;

                if output.len() > 15 * 1024 {
                    self.flush_response(&mut output)?;
                }
            }

            // flush response
            if !output.is_empty() {
                self.flush_response(&mut output)?;
            }

            // for next loop
            if pos == 0 {
                return Err(ParseError::InvalidHttp2("too long frame"));
            }
            if pos < end {
                input.copy_within(pos..end, 0);
                last_end = end - pos;
            } else {
                last_end = 0;
            }
        }
        Ok(())
    }

    fn handle_frame(&mut self, frame: &Frame, output: &mut Vec<u8>) -> Result<(), ParseError> {
        match frame.kind {
            FrameKind::Data => {
                self.req_data_len += frame.len;

                let req_buf = frame.process_data()?;

                // grpc-level-protocal
                if req_buf.len() == 0 {
                    return Ok(());
                }
                if req_buf.len() < 5 {
                    return Err(ParseError::InvalidHttp2("DATA frame too short for grpc"));
                }
                let req_buf = &req_buf[5..];

                // find the request-parse-fn
                let stream_id = frame.stream_id;
                let Some(parse_fn) = self.streams.remove(&stream_id) else {
                    return Err(ParseError::InvalidHttp2("DATA frame without HEADERS"));
                };

                let request = (parse_fn)(req_buf)?;

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
                let headers_buf = frame.process_headers()?;

                let parse_fn = self.hpack_decoder.find_path(headers_buf)?;

                if self.streams.insert(frame.stream_id, parse_fn).is_some() {
                    return Err(ParseError::InvalidHttp2("duplicated HEADERS frame"));
                }
            }
            _ => (),
        }

        Ok(())
    }

    fn flush_response(&mut self, output: &mut Vec<u8>) -> Result<(), ParseError> {
        build_window_update(self.req_data_len, output);

        self.c.write_all(output)?;

        output.clear();
        self.req_data_len = 0;
        Ok(())
    }
}
