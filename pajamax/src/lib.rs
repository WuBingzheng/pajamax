//! Super fast gRPC server framework in synchronous mode.
//!
//! I used and benchmarked the `tonic` in a network server project.
//! Surprisingly, I found that its performance is not as good as I expected,
//! with most of the cost being in the `tokio` asynchronous runtime and HTTP/2
//! protocol parsing. So I want to implement a higher-performance gRPC service
//! framework by solving the above two problems.
//!
//! # Optimization: Synchronous
//!
//! Asynchronous programming is very suitable for network applications. I love it,
//! but not here. `tokio` is fast but not zero-cost. For some gRPC servers in
//! certain scenarios, synchronous programming may be more appropriate:
//!
//! - Some business logic operates synchronously, allowing it to respond to
//!   requests immediately. Consequently, concurrent requests essentially
//!   line up in a pipeline here.
//!
//! - gRPC utilizes HTTP/2, which supports multiplexing. This means that even
//!   though each client can make multiple concurrent requests, only a single
//!   connection to the server is established. For internal services served
//!   behind a fixed number of gateway machines, the number of connections they
//!   handle remains limited and relatively small.
//!
//! In this case, the more straightforward **thread model** may be more suitable
//! than asynchronous model.
//! Spawn a thread for each connection. The code is synchronous inside each thread.
//! It receives requests and responds immediately, without employing `async` code
//! or any `tokio` components. Since the connections are very stable, there
//! is even no need to use a thread pool.
//!
//! # Optimization: Deep into HTTP/2
//!
//! gRPC runs over HTTP/2. gRPC and HTTP/2 are independent layers, and they SHOULD
//! also be independent in implementation, such as `tonic` and `h2` are two separate
//! crates. However, this independence also leads to performance waste, mainly
//! in the processing of request headers.
//!
//! - Typically, a standard HTTP/2 implementation must parse all request headers
//!   and return them to the upper-level application. But in a gRPC service, at
//!   least in specific scenarios, only the `:path` header is needed, while other
//!   headers can be ignored.
//!
//! - Even for the `:path` header, due to HPACK encoding, it needs allocate memory
//!   for an owned `String` before returning to the upper level to process. But
//!   in the specific scenario of gRPC, we can process directly on
//!   parsing `:path` in HTTP/2, thereby avoiding the memory allocation.
//!
//! To this end, we can implement an HTTP/2 library specifically designed for gRPC.
//! While "reducing coupling" is a golden rule in programming, there are exceptional
//! cases where it can be strategically overlooked for specific purposes.
//!
//! # Benchmark
//!
//! The above two optimizations eliminate the cost of the asynchronous runtime
//! and reduce the cost of HTTP/2 protocol parsing, resulting in a significant
//! performance improvement.
//!
//! We measured that Pajamax is up to 10X faster than Tonic using `grpc-bench`
//! project.  See the
//! [result](https://github.com/WuBingzheng/pajamax/blob/main/benchmark.md)
//! for details.
//!
//! # Conclusion
//!
//! Scenario limitations:
//!
//! - Synchronous business logical (but see the *Dispatch* mode below);
//! - Deployed in internal environment, behind the gateway and not directly exposed to the outside.
//!
//! Benefits:
//!
//! - 10X performance improvement at most;
//! - No asynchronous programming;
//! - Less dependencies, less compilation time, less executable size.
//!
//! Loss:
//!
//! - No gRPC Streaming mode, but only Unary mode;
//! - No gRPC headers, such as `grpc-timeout`;
//! - No `tower`'s ecosystem of middleware, services, and utilities, compared to `tonic`;
//! - maybe something else.
//!
//! It's like pajamas, super comfortable and convenient to wear, but only
//! suitable at home, not for going out in public.
//!
//! # Modes: Local and Dispatch
//!
//! The business logic code discussed above is all synchronous. There is
//! only one thread for each connection. We call it *Local* mode.
//! The architecture is very simple, as shown in the figure below.
//!
//! ```text
//!         /-----------------------\
//!        (      TCP connection     )
//!         \--^-----------------+--/
//!            |                 |
//!            |send             |recv
//!      +=====+=================V=====+
//!      |                             |
//!      |      application codes      |
//!      |                             |
//!      +===========pajamax framework=+
//! ```
//!
//! We also support another mode, *Dispatch* mode. This involves multiple threads:
//!
//! - one input thread, which receives TCP data, parses requests, and dispatches
//!   them to the specified backend threads according to user definitions;
//! - the backend threads are managed by the user themselves; They handle the
//!   requests and generate responses, just like in the *Local* mode;
//! - one output thread, which encodes responses and sends the data.
//!
//! The requests and responses are transfered by channels. The architecture is
//! shown in the figure below.
//!
//! ```text
//!         /-----------------------\
//!        (      TCP connection     )
//!         \--^-----------------+--/
//!            |                 |
//!            |send             |recv
//!     +======+=====+  +========V=======+
//!     | +----+---+ |  |  +----------+  |
//!     | | encode | |  |  | dispatch |  |
//!     | +-^----^-+ |  |  +--+----+--+  |
//!     |   |    :   |  |     :    |     |
//!     +===+====:===+  | +---V--+ |     |
//!         |    :      | |handle| |     |
//!         |    :      | +---+--+ |     |
//!         |    :      +=====:====+=====+
//!         |    :............:    |
//!      +==+======================V==+
//!      |                            |+
//!      |     application codes      ||+
//!      |                            |||
//!      +============================+||
//!       +============================+|
//!        +============================+
//! ```
//!
//! Applications can also decide some requests not to be dispatched, which
//! will be handled in the input-thread, just like in the *Local* mode.
//! But the responses have to be transfered to the output thread to sent.
//! As shown by the dashed line in the figure above.
//!
//! Applications only need to implement 2 traits to define how to dispatch
//! requests and how to handle requests. You do not need to handle the
//! message transfer or encoding, which will be handled by Pajamax.
//!
//! See the [dict-store](https://github.com/WuBingzheng/pajamax/blob/main/examples/src/dict_store.rs)
//! example for more details.
//!
//! # Usage
//! The usage of Pajamax is very similar to that of Tonic.
//!
//! See [`pajamax-build`](https://docs.rs/pajamax-build) crate document for more detail.
//!
//! # Status
//!
//! Now Pajamax is still in the development stage. I publish it to get feedback.
//!
//! Todo list:
//!
//! - More test;
//! - Configuration builder;
//! - Hooks like tower's Layer.

use std::net::{TcpListener, TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;

mod config;
mod connection;
mod error;
mod hpack_decoder;
mod hpack_encoder;
mod http2;
mod huffman;
mod local_server;
mod macros;
mod poll;
mod response_end;

pub mod dispatch_server;
pub mod status;

pub use config::Config;
pub use http2::RespEncode;

use dispatch_server::{DispatchConnection, PajamaxDispatchService};
use local_server::LocalConnection;

/// Wrapper of Result<Reply, Status>.
pub type Response<Reply> = Result<Reply, status::Status>;

/// Parse the request body from input data.
pub type ParseFn<R> = fn(&[u8]) -> Result<R, prost::DecodeError>;

/// Used by `pajamax-build` crate. It should implement this for service in .proto file.
pub trait PajamaxService {
    type Request;
    type Reply: RespEncode + Send + Sync + 'static;

    // On receiving a HEADERS frame, call this to locate the gRPC method
    // by `:path` header, and returns that method's request-parse-handler
    // which is used to parse the following DATA frame.
    fn request_parse_fn_by_path(path: &[u8]) -> Option<ParseFn<Self::Request>>;

    // Call methods' handlers on the request, and return response.
    fn call(&mut self, request: Self::Request) -> Response<Self::Reply>;
}

/*
/// Start server with default configurations, in local-mode.
pub fn serve_local<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: PajamaxService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    do_serve(
        |s, cnter, cfg| LocalConnection::new(srv.clone(), s, cnter, cfg),
        addr,
        Config::new(),
    )
}

/// Start server with default configurations, in dispatch-mode.
pub fn serve_dispatch<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: PajamaxDispatchService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    do_serve(
        |s, cnter, cfg| DispatchConnection::new(srv.clone(), s, cnter, cfg),
        addr,
        Config::new(),
    )
}
*/

/*
pub(crate) fn do_serve<S, F, A>(new_conn: F, addr: A, config: Config) -> std::io::Result<()>
where
    S: connection::ConnectionMode + Send + Sync + 'static,
    F: Fn(&TcpStream, Arc<AtomicUsize>, &Config) -> S + Send + Sync + 'static + Clone,
    A: ToSocketAddrs,
{
*/

pub fn serve_local<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: PajamaxService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    let counter = Arc::new(AtomicUsize::new(0));

    let mut workers = Vec::new();
    for _ in 0..4 {
        let (stream_tx, stream_rx) = mpsc::sync_channel(1000);
        let (waker_tx, waker_rx) = mpsc::sync_channel(1);

        let counter = counter.clone();
        let srv = srv.clone();
        thread::spawn(move || {
            poll::worker_thread(stream_rx, waker_tx, srv, counter, Config::new()).unwrap();
        });

        let waker = waker_rx.recv().unwrap();

        workers.push((stream_tx, waker));
    }

    let mut rri = 0;

    let listener = TcpListener::bind(addr)?;
    for c in listener.incoming() {
        //if counter.load(Ordering::Relaxed) >= config.max_concurrent_connections {
        //continue;
        //}
        println!("new conn");

        let (stream_tx, waker) = &workers[rri % 4];
        rri += 1;

        stream_tx.send(c?).unwrap();
        waker.wake().unwrap();
    }
    unreachable!();
}
