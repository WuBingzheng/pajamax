mod helloworld {
    include!(concat!(env!("OUT_DIR"), "/helloworld.rs"));
}

use helloworld::*;

struct MyGreeter();

impl Greeter for MyGreeter {
    fn say_hello(&self, req: HelloRequest) -> HelloReply {
        HelloReply {
            message: format!("Hello {}!", req.name),
        }
    }
}

fn main() {
    atiour::serve(GreeterServer::new(MyGreeter()), "127.0.0.1:50051").unwrap();
}
