#[allow(dead_code)]
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
