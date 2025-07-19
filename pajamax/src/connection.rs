use std::cell::RefCell;
use std::collections::VecDeque;
use std::io::Read;
use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use crate::config::Config;
use crate::dispatch;
use crate::error::Error;
use crate::hpack_decoder::{Decoder, PathKind};
use crate::http2::*;
use crate::response_end::ResponseEnd;
use crate::{PajamaxService, Response};

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
        // concurrent limit
        if concurrent.load(Ordering::Relaxed) >= config.max_concurrent_connections {
            // println!("drop connection"); // TODO add log
            continue;
        }
        concurrent.fetch_add(1, Ordering::Relaxed);

        // configure
        let c = c?;
        c.set_read_timeout(Some(config.idle_timeout))?;
        c.set_write_timeout(Some(config.write_timeout))?;

        // new thread for each connection
        let concurrent = concurrent.clone();
        let services = services.clone();
        thread::Builder::new()
            .name(String::from("pajamax-w"))
            .spawn(move || {
                let _ = handle(services, c, config); // TODO add log
                concurrent.fetch_sub(1, Ordering::Relaxed);
            })
            .unwrap();
    }
    unreachable!();
}

thread_local! {
    static RESPONSE_END: RefCell<ResponseEnd> = panic!();
}

struct Stream {
    id: u32,
    isvc: usize, // index of services
    req_disc: usize,
}

// response in local thread
pub fn local_build_response<Reply>(
    stream_id: u32,
    response: Response<Reply>,
    req_data_len: usize,
) -> Result<(), Error>
where
    Reply: prost::Message,
{
    RESPONSE_END
        .with_borrow_mut(|resp_end| Ok(resp_end.build(stream_id, response, req_data_len)?))
}

// handle each connection on a new thread
pub fn handle(
    services: Vec<Arc<dyn PajamaxService + Send + Sync + 'static>>,
    mut c: TcpStream,
    config: Config,
) -> Result<(), Error> {
    handshake(&mut c, &config)?;

    // prepare some contexts

    // network input buffer
    let mut input = Vec::new();
    input.resize(config.max_frame_size, 0);

    // stream info in HEADER frame
    let mut streams = VecDeque::new();

    let mut hpack_decoder: Decoder = Decoder::new();

    let mut route_cache = Vec::new();

    // split into 2 ends.
    // Read requests from `c` and write response into `c2`.
    // Wrap `Arc` for backend-response thread in dispatch-mode.
    let c2 = Arc::new(Mutex::new(c.try_clone()?));

    // create backend response thread if any dispatch-mode service
    if services.iter().any(|svc| svc.is_dispatch_mode()) {
        dispatch::new_response_routine(c2.clone(), &config);
    }

    // in local-mode, this writes all responses;
    // in dispatch-mode, this only writes dispatch-failure responses.
    RESPONSE_END.set(ResponseEnd::new(c2, &config));

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

            //println!("get frame: {:?}", frame); // TODO add log
            match frame.kind {
                // call ::route() with cache
                FrameKind::Headers => {
                    let headers_buf = frame.process_headers()?;

                    let (isvc, req_disc) = match hpack_decoder.find_path(headers_buf)? {
                        PathKind::Cached(cached) => route_cache[cached],
                        PathKind::Plain(path) => {
                            let len0 = route_cache.len();
                            for (i, svc) in services.iter().enumerate() {
                                if let Some(req_disc) = svc.route(&path) {
                                    route_cache.push((i, req_disc));
                                    break;
                                }
                            }
                            if route_cache.len() == len0 {
                                return Err(Error::UnknownMethod(
                                    String::from_utf8_lossy(&path).into(),
                                ));
                            }
                            route_cache[len0]
                        }
                    };

                    streams.push_back(Stream {
                        id: frame.stream_id,
                        isvc,
                        req_disc,
                    });
                }

                // call ::handle() to handle request
                FrameKind::Data => {
                    let req_buf = frame.process_data()?;

                    // unwrap grpc-level-protocal
                    if req_buf.len() == 0 {
                        continue;
                    }
                    if req_buf.len() < 5 {
                        return Err(Error::InvalidHttp2("DATA frame too short for grpc"));
                    }
                    let req_buf = &req_buf[5..];

                    // check out request info
                    let Some(i) = streams.iter().position(|s| s.id == frame.stream_id) else {
                        return Err(Error::InvalidHttp2("DATA frame without HEADER"));
                    };
                    let Stream { id, isvc, req_disc } = streams.remove(i).unwrap();

                    // handle request
                    services[isvc].handle(req_disc, req_buf, id, frame.len as usize)?;
                }
                _ => (),
            }
        }

        RESPONSE_END.with_borrow_mut(|resp_end| resp_end.flush())?;

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
