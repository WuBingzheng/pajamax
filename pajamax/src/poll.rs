//use std::io::{Read, Write};
use std::sync::atomic::AtomicUsize;
use std::sync::{mpsc, Arc};

use mio::net::TcpStream;
use mio::{Events, Interest, Poll, Token, Waker};
use slab::Slab;

use crate::config::Config;
use crate::connection::Connection;
use crate::error::Error;

const WAKER: usize = usize::MAX;

pub fn worker_thread<S>(
    stream_rx: mpsc::Receiver<std::net::TcpStream>,
    waker_tx: mpsc::SyncSender<Arc<Waker>>,
    //new_conn: F,
    srv: S,
    counter: Arc<AtomicUsize>,
    config: Config,
) -> std::io::Result<()>
where
    S: crate::PajamaxService + Clone + Send + Sync + 'static,
    //S: connection::ConnectionMode + Send + Sync + 'static,
    //F: Fn(&std::net::TcpStream, Arc<AtomicUsize>, &Config) -> S,
{
    let mut poll = Poll::new().unwrap();

    // waker for main thread to send stream
    let waker = Arc::new(Waker::new(poll.registry(), Token(WAKER))?);
    waker_tx.send(waker.clone()).unwrap();
    drop(waker_tx);

    let mut slab = Slab::new();

    let mut events = Events::with_capacity(128);
    loop {
        poll.poll(&mut events, None).unwrap();
        for event in events.iter() {
            match event.token().0 {
                WAKER => {
                    while let Ok(stream) = stream_rx.try_recv() {
                        //let srv_conn = new_conn(&stream, counter.clone(), &config);
                        let srv_conn = crate::local_server::LocalConnection::new(
                            srv.clone(),
                            &stream,
                            counter.clone(),
                            &config,
                        );

                        stream.set_nonblocking(true)?;
                        let mut stream = TcpStream::from_std(stream);
                        poll.registry()
                            .register(&mut stream, Token(slab.vacant_key()), Interest::READABLE)
                            .unwrap();

                        let connection = Connection::new(srv_conn, stream, &config);
                        slab.insert(connection);
                    }
                }
                token => {
                    //let (srv_conn, stream) = slab.get_mut(token).unwrap();
                    let connection = slab.get_mut(token).unwrap();
                    match connection.handle() {
                        Ok(0) => {
                            slab.remove(token);
                        }
                        Ok(_) => {}
                        Err(Error::IoFail(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(e) => {
                            eprintln!("Error: {:?}", e);
                            slab.remove(token);
                        }
                    }
                }
            }
        }
    }
}
