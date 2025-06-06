#[derive(Debug)]
pub enum Error {
    InvalidHttp2(&'static str),
    InvalidHpack(&'static str),
    InvalidHuffman,
    InvalidProtobuf(prost::DecodeError),
    IoFail(std::io::Error),
    ChannelClosed,
    UnknownMethod(String),
    NoPathSet,
}

impl From<std::io::Error> for Error {
    fn from(io: std::io::Error) -> Self {
        Self::IoFail(io)
    }
}

impl From<std::sync::mpsc::RecvError> for Error {
    fn from(_: std::sync::mpsc::RecvError) -> Self {
        Self::ChannelClosed
    }
}

impl From<prost::DecodeError> for Error {
    fn from(de: prost::DecodeError) -> Self {
        Self::InvalidProtobuf(de)
    }
}

use std::fmt;
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidHttp2(s) => write!(f, "invalid http2: {s}"),
            Error::InvalidHpack(s) => write!(f, "invalid hpack: {s}"),
            Error::InvalidHuffman => write!(f, "invalid huffman"),
            Error::InvalidProtobuf(e) => write!(f, "invalid protobuf: {e}"),
            Error::IoFail(e) => write!(f, "IO fail: {e}"),
            Error::ChannelClosed => write!(f, "channel closed"),
            Error::UnknownMethod(m) => write!(f, "unknown method: {m}"),
            Error::NoPathSet => write!(f, "no :path set"),
        }
    }
}

impl std::error::Error for Error {}
