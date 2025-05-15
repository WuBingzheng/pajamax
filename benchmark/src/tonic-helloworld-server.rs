use tonic::{transport::Server, Request, Response, Status};

use helloworld::greeter_server::{Greeter, GreeterServer};
use helloworld::{HelloReply, HelloRequest};

pub mod helloworld{
    tonic::include_proto!("helloworld");
}

#[derive(Default)]
pub struct MyGreeter {}

#[tonic::async_trait]
impl Greeter for MyGreeter {
    async fn say_hello(
        &self,
        request: Request<HelloRequest>,
    ) -> Result<Response<HelloReply>, Status> {
        //println!("Got a request from {:?}", request.remote_addr());

        let reply = helloworld::HelloReply {
            message: format!("Hello {}!", request.into_inner().name),
        };
        Ok(Response::new(reply))
    }
}

async fn start_server() {
    let addr = "127.0.0.1:50051".parse().unwrap();
    let greeter = MyGreeter::default();

    println!("GreeterServer listening on {addr}");

    Server::builder()
        .add_service(GreeterServer::new(greeter))
        .serve(addr)
        .await
        .unwrap();
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 2 {
        println!("Usage: {} threads", args[0]);
        return;
    }
    let threads: usize = args[1].parse().unwrap();

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(threads)
        .enable_io()
        .enable_time()
        .build()
        .unwrap()
        .block_on(start_server());
}
