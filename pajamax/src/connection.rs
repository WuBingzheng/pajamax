use std::collections::HashMap;
use std::io::Read;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::Config;
//use crate::dispatch::DispatchCtx;
use crate::error::Error;
use crate::hpack_decoder::Decoder;
use crate::http2::*;
use crate::response_end::ResponseEnd;
use crate::PajamaxService;

pub fn serve_with_config<A>(
    services: Vec<Arc<dyn PajamaxService + Send + Sync + 'static>>,
    config: Config,
    addr: A,
) -> std::io::Result<()>
where
    A: ToSocketAddrs,
{
    let concurrent = Arc::new(AtomicUsize::new(0));

    let listener = TcpListener::bind(addr)?;
    for c in listener.incoming() {
        if concurrent.load(Ordering::Relaxed) >= config.max_concurrent_connections {
            continue;
        }
        concurrent.fetch_add(1, Ordering::Relaxed);

        let c = c?;
        c.set_read_timeout(Some(config.idle_timeout))?;
        c.set_write_timeout(Some(config.write_timeout))?;

        let concurrent = concurrent.clone();
        let services = services.clone();
        thread::Builder::new()
            .name(String::from("pajamax-w"))
            .spawn(move || {
                let _ = handle2(services, c, config);
                concurrent.fetch_sub(1, Ordering::Relaxed);
            })
            .unwrap();
    }
    unreachable!();
}

pub fn handle2(
    services: Vec<Arc<dyn PajamaxService + Send + Sync + 'static>>,
    mut c: TcpStream,
    config: Config,
) -> Result<(), Error> {
    handshake(&mut c, &config)?;

    // prepare some contexts
    let mut input = Vec::new();
    input.resize(config.max_frame_size, 0);

    let mut streams: HashMap<u32, String> = HashMap::new();
    let mut hpack_decoder: Decoder = Decoder::new();

    let c2 = Arc::new(Mutex::new(c.try_clone()?)); // output end
    let mut resp_end = ResponseEnd::new(c2.clone(), &config);

    // read and parse input data
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

                    // find the req-disc
                    let stream_id = frame.stream_id;
                    let Some(req_disc) = streams.remove(&stream_id) else {
                        return Err(Error::InvalidHttp2("DATA frame without HEADERS"));
                    };

                    services[0].handle(
                        &req_disc,
                        req_buf,
                        stream_id,
                        frame.len as usize,
                        &mut resp_end,
                    );
                }
                FrameKind::Headers => {
                    let headers_buf = frame.process_headers()?;

                    let req_disc = hpack_decoder.find_path(headers_buf)?;

                    if streams.insert(frame.stream_id, req_disc).is_some() {
                        return Err(Error::InvalidHttp2("duplicated HEADERS frame"));
                    }
                }
                _ => (),
            }
        }

        resp_end.flush(true)?;

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
