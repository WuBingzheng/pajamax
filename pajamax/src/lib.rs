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
//! In my benchmark, Pajamax, which is implemented based on the above optimizations,
//! is 10X faster than Tonic.
//! See [the benchmark](https://github.com/WuBingzheng/pajamax/tree/main/benchmark)
//! for the code and more details.
//!
//! # Conclusion
//!
//! Scenario limitations:
//!
//! - Your business must be synchronous;
//! - Deployed in internal environment, behind the gateway and not directly exposed to the outside.
//!
//! Benefits:
//!
//! - 10x performance improvement;
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
//! # Usage
//!
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
//! - Hooks like tower's layers.
//! - A new mode, under which network threads send the received requests to other
//!   threads for processing via a channel.
//!
//! # License
//!
//! MIT

use std::net::{TcpListener, ToSocketAddrs};
use std::thread;

mod connection;
mod hpack_decoder;
mod hpack_encoder;
mod http2;
mod huffman;
pub mod status;

use crate::connection::Connection;
use crate::status::Status;

/// Parse the request body from input data.
pub type ParseFn<R> = fn(&[u8]) -> Result<R, prost::DecodeError>;

/// Used by `pajamax-build` crate. It should implement this for service in .proto file.
pub trait PajamaxService {
    type Request;

    // On receiving a HEADERS frame, call this to locate the gRPC method
    // by `:path` header, and returns that method's request-parse-handler
    // which is used to parse the following DATA frame.
    fn request_parse_fn_by_path(path: &[u8]) -> Option<ParseFn<Self::Request>>;

    // Call methods' handlers on the request, and return response.
    fn call(&self, request: Self::Request) -> Result<impl prost::Message, Status>;
}

/// Start the server.
pub fn serve<S, A>(srv: S, addr: A) -> std::io::Result<()>
where
    S: PajamaxService + Clone + Send + Sync + 'static,
    A: ToSocketAddrs,
{
    let listener = TcpListener::bind(addr)?;
    for connection in listener.incoming() {
        let c = Connection::new(connection?, srv.clone());
        thread::spawn(move || c.handle());
    }
    unreachable!();
}

/// Include generated proto server and client items.
///
/// You must specify the gRPC package name.
///
/// Examples:
///
/// ```rust,ignore
/// mod pb {
///     pajamax::include_proto!("helloworld");
/// }
/// ```
#[macro_export]
macro_rules! include_proto {
    ($package: tt) => {
        include!(concat!(env!("OUT_DIR"), concat!("/", $package, ".rs")));
    };
}
