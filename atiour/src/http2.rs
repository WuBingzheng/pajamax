use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;

use loona_hpack::{Decoder, Encoder};

use log::*;

#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FrameKind {
    Data = 0,
    Headers = 1,
    Priority = 2,
    Reset = 3,
    Settings = 4,
    PushPromise = 5,
    Ping = 6,
    GoAway = 7,
    WindowUpdate = 8,
    Continuation = 9,
    Unknown,
}

impl FrameKind {
    pub fn from(byte: u8) -> Self {
        match byte {
            0 => FrameKind::Data,
            1 => FrameKind::Headers,
            2 => FrameKind::Priority,
            3 => FrameKind::Reset,
            4 => FrameKind::Settings,
            5 => FrameKind::PushPromise,
            6 => FrameKind::Ping,
            7 => FrameKind::GoAway,
            8 => FrameKind::WindowUpdate,
            9 => FrameKind::Continuation,
            _ => FrameKind::Unknown,
        }
    }
}

#[derive(Debug)]
struct FrameHead {
    len: usize,
    flags: HeadFlags,
    kind: FrameKind,
    stream_id: u32,
}

fn parse_u32(buf: &[u8]) -> u32 {
    let tmp: [u8; 4] = [buf[0], buf[1], buf[2], buf[3]];
    u32::from_be_bytes(tmp)
}
fn parse_u64(buf: &[u8]) -> u64 {
    let mut tmp: [u8; 8] = [0; 8];
    tmp.copy_from_slice(&buf[..8]);
    u64::from_be_bytes(tmp)
}

impl FrameHead {
    const SIZE: usize = 9;

    fn parse(buf: &[u8]) -> Option<Self> {
        if buf.len() < Self::SIZE {
            return None;
        }

        let tmp: [u8; 4] = [0, buf[0], buf[1], buf[2]];
        let len = u32::from_be_bytes(tmp) as usize;
        if buf.len() - Self::SIZE < len {
            return None;
        }

        Some(Self {
            len,
            kind: FrameKind::from(buf[3]),
            flags: HeadFlags::from(buf[4]),
            stream_id: parse_u32(&buf[5..]),
        })
    }

    fn build(len: usize, kind: FrameKind, flags: u8, stream_id: u32, output: &mut [u8]) {
        let tmp = (len as u32).to_be_bytes();
        output[..3].copy_from_slice(&tmp[1..]);

        output[3] = kind as u8;
        output[4] = flags;

        let tmp = stream_id.to_be_bytes();
        output[5..9].copy_from_slice(&tmp);
    }

    fn skip_padded<'a, 'b>(&'a self, buf: &'b [u8]) -> Option<&'b [u8]> {
        if self.flags.is_padded() {
            if buf.len() < 1 {
                info!("invalid frame for padded");
                return None;
            }
            let pad_len = buf[0] as usize;
            let buf_len = buf.len();
            if buf_len <= 1 + pad_len {
                info!("invalid frame for padded");
                return None;
            }
            Some(&buf[1..buf_len - pad_len])
        } else {
            Some(buf)
        }
    }

    fn skip_priority<'a, 'b>(&'a self, buf: &'b [u8]) -> Option<&'b [u8]> {
        if self.flags.is_priority() {
            if buf.len() < 5 {
                info!("invalid frame for padded");
                return None;
            }
            Some(&buf[5..])
        } else {
            Some(buf)
        }
    }
}

pub trait AtyourService {
    type Request;
    fn request_parse_fn_by_path(
        path: &[u8],
    ) -> Option<fn(&[u8]) -> Result<Self::Request, prost::DecodeError>>;
    fn call(&self, request: Self::Request) -> impl prost::Message;
}

struct Stream<S: AtyourService> {
    id: u32,
    request_parse_fn: fn(&[u8]) -> Result<S::Request, prost::DecodeError>,
}

pub fn handle_connection<S: AtyourService>(mut connection: TcpStream, srv: S) {
    if !handshake(&mut connection) {
        return;
    }

    let mut hpack_decoder = Decoder::new();
    let mut hpack_encoder = Encoder::new();

    let mut input = Vec::new();
    input.resize(16 * 1024, 0);

    let mut output = Vec::new();
    output.resize(16 * 1024, 0);

    let mut streams: HashMap<u32, Stream<S>> = HashMap::new();

    let mut last_end = 0;
    while let Ok(len) = connection.read(&mut input[last_end..]) {
        if len == 0 {
            trace!("connection closed");
            break;
        }
        let end = last_end + len;

        //let mut x_method: Option<fn(&[u8]) -> Result<GreeterMethod, prost::DecodeError>> = None;

        let mut pos = 0;
        while let Some(frame_head) = FrameHead::parse(&input[pos..end]) {
            let payload_start = pos + FrameHead::SIZE;
            let payload_end = payload_start + frame_head.len;
            let payload = &input[payload_start..payload_end];
            pos = payload_end;

            match frame_head.kind {
                FrameKind::Data => {
                    if let Some(req_buf) = process_data(&frame_head, payload) {
                        let Some(stream) = streams.remove(&frame_head.stream_id) else {
                            info!("DATA frame without HEADERS");
                            break;
                        };

                        let Ok(request) = (stream.request_parse_fn)(req_buf) else {
                            info!("fail in parse request");
                            break;
                        };

                        let reply = srv.call(request);

                        build_response(stream.id, reply, &mut hpack_encoder, &mut output);

                        if connection.write_all(&output).is_err() {
                            info!("connection send error");
                            break;
                        }
                    }
                }
                FrameKind::Headers => {
                    let Some(request_parse_fn) =
                        process_headers::<S>(&frame_head, payload, &mut hpack_decoder)
                    else {
                        break;
                    };

                    let stream = Stream {
                        id: frame_head.stream_id,
                        request_parse_fn,
                    };

                    if streams.insert(frame_head.stream_id, stream).is_some() {
                        info!("duplicate HEADERS frame");
                        break;
                    }
                }
                _ => (), //println!("unknown frame: {:?}", head.kind),
            }
        }

        if pos == 0 {
            error!("too long frame, we current support 16K by now.");
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

fn handshake(stream: &mut TcpStream) -> bool {
    let mut input = vec![0; 24];
    let Ok(len) = stream.read(&mut input) else {
        info!("read fail at handshake");
        return false;
    };
    if len != 24 {
        info!("too short handshake: {len}");
        return false;
    }
    if input != *b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n" {
        info!("invalid handshake message ({len}): {:?}", &input);
        return false;
    }
    true
}

#[derive(Debug, Copy, Clone)]
struct HeadFlags(u8);
impl HeadFlags {
    const END_STREAM: u8 = 0x1;
    const END_HEADERS: u8 = 0x4;
    const PADDED: u8 = 0x8;
    const PRIORITY: u8 = 0x20;

    fn from(flag: u8) -> Self {
        Self(flag)
    }
    fn is_end_stream(self) -> bool {
        self.0 & Self::END_STREAM != 0
    }
    fn is_end_headers(self) -> bool {
        self.0 & Self::END_HEADERS != 0
    }
    fn is_padded(self) -> bool {
        self.0 & Self::PADDED != 0
    }
    fn is_priority(self) -> bool {
        self.0 & Self::PRIORITY != 0
    }
}

fn process_headers<S: AtyourService>(
    frame_head: &FrameHead,
    input: &[u8],
    hpack_decoder: &mut Decoder,
) -> Option<fn(&[u8]) -> Result<S::Request, prost::DecodeError>> {
    if !frame_head.flags.is_end_headers() {
        error!("we do not support multiple HEADERS frames for one frame");
        return None;
    }
    if frame_head.flags.is_end_stream() {
        error!("expect DATA frame");
        return None;
    }
    let input = frame_head.skip_padded(input)?;
    let input = frame_head.skip_priority(input)?;

    let mut request_parse_fn = None;
    hpack_decoder
        .decode_with_cb(input, |key, value| {
            if key.as_ref() == b":path" {
                let path = value.as_ref();
                trace!("read path: {:?}", std::str::from_utf8(path));
                request_parse_fn = S::request_parse_fn_by_path(path);
            }
        })
        .ok()?;

    request_parse_fn
}

fn process_data<'a, 'b>(frame_head: &'a FrameHead, buf: &'b [u8]) -> Option<&'b [u8]> {
    let buf = frame_head.skip_padded(buf)?;

    if frame_head.len == 0 {
        None
    } else if frame_head.len < 5 {
        info!("not complete grpc message header");
        None
    } else {
        Some(&buf[5..])
    }
}

fn build_response<M: prost::Message>(
    stream_id: u32,
    reply: M,
    hpack_encoder: &mut Encoder,
    output: &mut Vec<u8>,
) {
    // HEADERS
    let start = output.len();
    output.resize(start + FrameHead::SIZE, 0);
    hpack_encoder
        .encode_header_into((b"status", b"200"), output)
        .unwrap();

    FrameHead::build(
        output.len() - start - FrameHead::SIZE,
        FrameKind::Headers,
        HeadFlags::END_HEADERS,
        stream_id,
        output,
    );

    // DATA
    let data_start = output.len();
    let payload_start = data_start + FrameHead::SIZE;
    let msg_start = payload_start + 5;
    output.resize(msg_start, 0);

    reply.encode(output).unwrap();

    let msg_len = output.len() - msg_start;
    let payload_len = msg_len + 5;

    FrameHead::build(
        payload_len,
        FrameKind::Data,
        HeadFlags::END_STREAM,
        stream_id,
        &mut output[data_start..],
    );

    let tmp = (msg_len as u32).to_be_bytes();
    output[payload_start + 1..payload_start + 5].copy_from_slice(&tmp);
}
