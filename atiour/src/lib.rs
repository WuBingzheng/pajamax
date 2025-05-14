use std::net::{TcpListener, ToSocketAddrs};
use std::thread;

use log::*;

mod connection;
mod hpack_decoder;
mod hpack_encoder;
mod http2;
mod huffman;
pub mod status;

use crate::connection::Connection;
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

pub fn serve<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: AtiourService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    let listener = TcpListener::bind(addr)?;
    for connection in listener.incoming() {
        trace!("new connection");
        let c = Connection::new(connection?, srv.clone());
        thread::spawn(move || c.handle());
    }
    unreachable!();
}
