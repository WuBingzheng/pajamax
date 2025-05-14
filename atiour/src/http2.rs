use std::io::{Read, Write};
use std::net::TcpStream;

use log::*;

use crate::hpack_encoder::Encoder;
use crate::status::Status;

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
pub struct Frame<'a> {
    pub len: usize,
    pub flags: HeadFlags,
    pub kind: FrameKind,
    pub stream_id: u32,
    pub payload: &'a [u8],
}

impl<'a> Frame<'a> {
    pub const HEAD_SIZE: usize = 9;

    pub fn parse(buf: &'a [u8]) -> Option<Self> {
        if buf.len() < Self::HEAD_SIZE {
            return None;
        }

        let tmp: [u8; 4] = [0, buf[0], buf[1], buf[2]];
        let len = u32::from_be_bytes(tmp) as usize;
        if buf.len() - Self::HEAD_SIZE < len {
            return None;
        }

        Some(Self {
            len,
            kind: FrameKind::from(buf[3]),
            flags: HeadFlags::from(buf[4]),
            stream_id: parse_u32(&buf[5..]),
            payload: &buf[Frame::HEAD_SIZE..Frame::HEAD_SIZE + len],
        })
    }

    fn build_head(len: usize, kind: FrameKind, flags: u8, stream_id: u32, output: &mut [u8]) {
        let tmp = (len as u32).to_be_bytes();
        output[..3].copy_from_slice(&tmp[1..]);

        output[3] = kind as u8;
        output[4] = flags;

        build_u32(stream_id, &mut output[5..9]);
    }

    pub fn process_headers(&self) -> Option<&[u8]> {
        if !self.flags.is_end_headers() {
            error!("we do not support multiple HEADERS frames for one frame");
            return None;
        }
        if self.flags.is_end_stream() {
            error!("expect DATA frame");
            return None;
        }
        let headers = self.skip_padded(self.payload)?;
        let headers = self.skip_priority(headers)?;

        Some(headers)
    }

    pub fn process_data(&self) -> Option<&[u8]> {
        let data = self.skip_padded(self.payload)?;
        Some(data)
    }

    fn skip_padded<'b>(&self, buf: &'b [u8]) -> Option<&'b [u8]> {
        if self.flags.is_padded() {
            if buf.len() < 1 {
                warn!("invalid frame for padded");
                return None;
            }
            let pad_len = buf[0] as usize;
            let buf_len = buf.len();
            if buf_len <= 1 + pad_len {
                warn!("invalid frame for padded");
                return None;
            }
            Some(&buf[1..buf_len - pad_len])
        } else {
            Some(buf)
        }
    }

    fn skip_priority<'b>(&self, buf: &'b [u8]) -> Option<&'b [u8]> {
        if self.flags.is_priority() {
            if buf.len() < 5 {
                warn!("invalid frame for padded");
                return None;
            }
            Some(&buf[5..])
        } else {
            Some(buf)
        }
    }
}

pub fn handshake(connection: &mut TcpStream) -> bool {
    // parse the magic
    let mut input = vec![0; 24];
    let Ok(len) = connection.read(&mut input) else {
        warn!("read fail at handshake");
        return false;
    };
    if len != 24 {
        warn!("too short handshake: {len}");
        return false;
    }
    if input != *b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n" {
        warn!("invalid handshake message ({len}): {:?}", &input);
        return false;
    }

    // send empty SETTINGS
    // TODO
    let mut output = Vec::new();
    output.resize(9, 0);
    Frame::build_head(0, FrameKind::Settings, 0, 0, &mut output);
    let Ok(_) = connection.write_all(&output) else {
        warn!("send fail at handshake");
        return false;
    };

    true
}

#[derive(Debug, Copy, Clone)]
pub struct HeadFlags(u8);
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

pub fn build_response<M: prost::Message>(
    stream_id: u32,
    reply: M,
    hpack_encoder: &mut Encoder,
    output: &mut Vec<u8>,
) {
    // HEADERS
    let start = output.len();
    output.resize(start + Frame::HEAD_SIZE, 0);
    hpack_encoder.encode_status_200(output);
    hpack_encoder.encode_grpc_status_zero(output);

    Frame::build_head(
        output.len() - start - Frame::HEAD_SIZE,
        FrameKind::Headers,
        HeadFlags::END_HEADERS,
        stream_id,
        &mut output[start..],
    );

    // DATA
    let data_start = output.len();
    let payload_start = data_start + Frame::HEAD_SIZE;
    let msg_start = payload_start + 5;
    output.resize(msg_start, 0);

    reply.encode(output).unwrap();

    let msg_len = output.len() - msg_start;
    let payload_len = msg_len + 5;

    Frame::build_head(
        payload_len,
        FrameKind::Data,
        HeadFlags::END_STREAM,
        stream_id,
        &mut output[data_start..],
    );

    build_u32(
        msg_len as u32,
        &mut output[payload_start + 1..payload_start + 5],
    );
}

pub fn build_status(
    stream_id: u32,
    status: Status,
    hpack_encoder: &mut Encoder,
    output: &mut Vec<u8>,
) {
    // HEADERS
    let start = output.len();
    output.resize(start + Frame::HEAD_SIZE, 0);
    hpack_encoder.encode_status_200(output);
    hpack_encoder.encode_grpc_status_nonzero(status.code as usize, output);
    hpack_encoder.encode_grpc_message(&status.message, output);

    Frame::build_head(
        output.len() - start - Frame::HEAD_SIZE,
        FrameKind::Headers,
        HeadFlags::END_HEADERS,
        stream_id,
        &mut output[start..],
    );
}

pub fn build_window_update(len: usize, output: &mut Vec<u8>) {
    let start = output.len();
    output.resize(start + Frame::HEAD_SIZE + 4, 0);

    Frame::build_head(4, FrameKind::WindowUpdate, 0, 0, &mut output[start..]);

    build_u32(len as u32, &mut output[start + Frame::HEAD_SIZE..]);
}

fn parse_u32(buf: &[u8]) -> u32 {
    let tmp: [u8; 4] = [buf[0], buf[1], buf[2], buf[3]];
    u32::from_be_bytes(tmp)
}
fn build_u32(n: u32, buf: &mut [u8]) {
    let tmp = n.to_be_bytes();
    buf.copy_from_slice(&tmp);
}
