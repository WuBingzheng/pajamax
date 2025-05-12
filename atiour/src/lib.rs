use std::net::{TcpListener, ToSocketAddrs};
use std::thread;

mod http2;
use http2::handle_connection;

pub trait AtiourService {
    type Request;
    fn request_parse_fn_by_path(
        path: &[u8],
    ) -> Option<fn(&[u8]) -> Result<Self::Request, prost::DecodeError>>;
    fn call(&self, request: Self::Request) -> impl prost::Message;
}

pub fn serve<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: AtiourService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    let listener = TcpListener::bind(addr)?;
    for connection in listener.incoming() {
        let connection = connection?;
        let srv = srv.clone();
        thread::spawn(move || handle_connection(connection, srv));
    }
    unreachable!();
}
