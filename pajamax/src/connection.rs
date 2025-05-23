use std::collections::HashMap;
use std::io::Read;
use std::net::TcpStream;

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

pub fn handle<S>(mut srv_conn: S, mut c: TcpStream, config: Config) -> Result<(), Error>
where
    S: ConnectionMode,
{
    handshake(&mut c, &config)?;

    let mut input = Vec::new();
    input.resize(config.max_frame_size, 0);

    let mut streams: HashMap<u32, ParseFn<<S::Service as PajamaxService>::Request>> =
        HashMap::new();
    let mut hpack_decoder: Decoder<S::Service> = Decoder::new();

    let mut last_end = 0;
    while let Ok(len) = c.read(&mut input[last_end..]) {
        if len == 0 {
            // connection closed
            return Ok(());
        }
        let end = last_end + len;

        let mut pos = 0;
        while let Some(frame) = Frame::parse(&input[pos..end]) {
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
                    let Some(parse_fn) = streams.remove(&stream_id) else {
                        return Err(Error::InvalidHttp2("DATA frame without HEADERS"));
                    };

                    let request = (parse_fn)(req_buf)?;

                    // call the method!
                    srv_conn.handle_call(request, stream_id, frame.len)?;
                }
                FrameKind::Headers => {
                    let headers_buf = frame.process_headers()?;

                    let parse_fn = hpack_decoder.find_path(headers_buf)?;

                    if streams.insert(frame.stream_id, parse_fn).is_some() {
                        return Err(Error::InvalidHttp2("duplicated HEADERS frame"));
                    }
                }
                _ => (),
            }
        }

        srv_conn.defer_flush()?;

        // for next loop
        if pos == 0 {
            return Err(Error::InvalidHttp2("too long frame"));
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
