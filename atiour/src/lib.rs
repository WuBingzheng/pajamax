use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::thread;

use log::*;

mod hpack_decoder;
mod hpack_encoder;
mod http2;
mod huffman;
pub mod status;

use crate::http2::*;
use crate::status::Status;

pub type ParseFn<R> = fn(&[u8]) -> Result<R, prost::DecodeError>;

// `atiour-build` crate should implement this for service in .proto file.
pub trait AtiourService {
    type Request;

    // On receiving a HEADERS frame, call this to locate the gRPC method
    // by `:path` header, and returns that method's request-parse-handler
    // which is used to parse the following DATA frame.
    fn request_parse_fn_by_path(path: &[u8]) -> Option<ParseFn<Self::Request>>;

    // Call methods' handlers on the request, and return response.
    fn call(&self, request: Self::Request) -> Result<impl prost::Message, Status>;
}

fn handle_connection<S: AtiourService>(mut connection: TcpStream, srv: S) {
    if !handshake(&mut connection) {
        return;
    }

    let mut hpack_decoder = hpack_decoder::Decoder::new();
    let mut hpack_encoder = hpack_encoder::Encoder::new();

    let mut input = Vec::new();
    input.resize(16 * 1024, 0);

    let mut output = Vec::with_capacity(16 * 1024);

    let mut streams: HashMap<u32, ParseFn<S::Request>> = HashMap::new();

    let mut last_end = 0;
    while let Ok(len) = connection.read(&mut input[last_end..]) {
        if len == 0 {
            trace!("connection closed");
            break;
        }
        let end = last_end + len;

        let mut req_data_len = 0; // for WINDOW_UPDATE

        let mut pos = 0;
        while let Some(frame) = Frame::parse(&input[pos..end]) {
            pos += Frame::HEAD_SIZE + frame.len; // for next loop

            match frame.kind {
                FrameKind::Data => {
                    req_data_len += frame.len;

                    let Some(req_buf) = frame.process_data() else {
                        continue; // empty DATA with END_STREAM flag
                    };

                    // grpc-level-protocal
                    if req_buf.len() == 0 {
                        continue;
                    }
                    if req_buf.len() < 5 {
                        warn!("DATA frame invalid grpc-protocal");
                        return;
                    }
                    let req_buf = &req_buf[5..];

                    // find the request-parse-fn
                    let stream_id = frame.stream_id;
                    let Some(parse_fn) = streams.remove(&stream_id) else {
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
                    match srv.call(request) {
                        Ok(reply) => {
                            build_response(stream_id, reply, &mut hpack_encoder, &mut output);
                        }
                        Err(status) => {
                            build_status(stream_id, status, &mut hpack_encoder, &mut output);
                        }
                    }

                    // flush response
                    if output.len() > output.capacity() * 9 / 10 {
                        build_window_update(req_data_len, &mut output);
                        if let Err(err) = connection.write_all(&output) {
                            info!("connection send fail: {:?}", err);
                            return;
                        }
                        output.clear();
                        req_data_len = 0;
                    }
                }
                FrameKind::Headers => {
                    let Some(headers_buf) = frame.process_headers() else {
                        return;
                    };

                    let parse_fn =
                        match hpack_decoder.find_path(headers_buf, S::request_parse_fn_by_path) {
                            Ok(parse_fn) => parse_fn,
                            Err(err) => {
                                warn!("fain in find path: {:?}", err);
                                return;
                            }
                        };

                    if streams.insert(frame.stream_id, parse_fn).is_some() {
                        info!("duplicate HEADERS frame");
                        return;
                    }
                }
                k => trace!("omit other frames: {:?}", k),
            }
        }

        // flush response
        if req_data_len != 0 || !output.is_empty() {
            build_window_update(req_data_len, &mut output);
            if let Err(err) = connection.write_all(&output) {
                info!("connection send fail: {:?}", err);
                return;
            }
            output.clear();
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

pub fn serve<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: AtiourService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    let listener = TcpListener::bind(addr)?;
    for connection in listener.incoming() {
        trace!("new connection");
        let connection = connection?;
        let srv = srv.clone();
        thread::spawn(move || handle_connection(connection, srv));
    }
    unreachable!();
}
