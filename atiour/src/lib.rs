use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::thread;

use log::*;
use loona_hpack::{Decoder, Encoder};

mod http2;

use crate::http2::*;

type ParseFn<R> = fn(&[u8]) -> Result<R, prost::DecodeError>;

// `atiour-build` crate should implement this for service in .proto file.
pub trait AtiourService {
    type Request;

    // On receiving a HEADERS frame, call this to locate the gRPC method
    // by `:path` header, and returns that method's request-parse-handler
    // which is used to parse the following DATA frame.
    fn request_parse_fn_by_path(path: &[u8]) -> Option<ParseFn<Self::Request>>;

    // Call methods' handlers on the request, and return response.
    fn call(&self, request: Self::Request) -> impl prost::Message;
}

fn handle_connection<S: AtiourService>(mut connection: TcpStream, srv: S) {
    if !handshake(&mut connection) {
        return;
    }

    let mut hpack_decoder = Decoder::new();
    let mut hpack_encoder = Encoder::new();

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

        let mut pos = 0;
        while let Some(frame_head) = FrameHead::parse(&input[pos..end]) {
            let payload_start = pos + FrameHead::SIZE;
            let payload_end = payload_start + frame_head.len;
            let payload = &input[payload_start..payload_end];
            pos = payload_end; // for next loop

            match frame_head.kind {
                FrameKind::Data => {
                    let Some(req_buf) = process_data(&frame_head, payload) else {
                        continue; // empty DATA with END_STREAM flag
                    };
                    let Some(parse_fn) = streams.remove(&frame_head.stream_id) else {
                        warn!("DATA frame without HEADERS");
                        break;
                    };

                    let request = match (parse_fn)(req_buf) {
                        Ok(request) => request,
                        Err(err) => {
                            warn!("fail in parse request: {:?}", err);
                            break;
                        }
                    };

                    let reply = srv.call(request);

                    build_response(&frame_head, reply, &mut hpack_encoder, &mut output);

                    if let Err(err) = connection.write_all(&output) {
                        warn!("connection send error: {:?}", err);
                        break;
                    }
                    output.clear();
                }
                FrameKind::Headers => {
                    let Some(headers_buf) = process_headers(&frame_head, payload) else {
                        break;
                    };

                    let mut parse_fn = None;
                    if let Err(err) = hpack_decoder.decode_with_cb(headers_buf, |key, value| {
                        if key.as_ref() == b":path" {
                            let path = value.as_ref();
                            trace!("read path: {:?}", std::str::from_utf8(path));
                            parse_fn = S::request_parse_fn_by_path(path);
                        }
                    }) {
                        warn!("fail in decode hpack: {:?}", err);
                        break;
                    }

                    let Some(parse_fn) = parse_fn else {
                        warn!("miss :path header");
                        break;
                    };

                    if streams.insert(frame_head.stream_id, parse_fn).is_some() {
                        info!("duplicate HEADERS frame");
                        break;
                    }
                }
                k => trace!("omit other frames: {:?}", k),
            }
        }

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
