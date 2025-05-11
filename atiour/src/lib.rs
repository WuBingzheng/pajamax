use std::net::{TcpListener, ToSocketAddrs};
use std::thread;

mod http2;
use http2::{handle_connection, AtyourService};

pub fn serve<S: AtyourService, A: ToSocketAddrs>(srv: S, addr: A) -> std::io::Result<()> {
    let listener = TcpListener::bind(addr)?;
    for connection in listener.incoming() {
        let srv = srv.clone();
        thread::spawn(move || handle_connection(connection?, srv));
    }
    unreachable!();
}
