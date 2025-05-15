use helloworld::greeter_client::GreeterClient;
use helloworld::HelloRequest;
use std::time::Instant;

pub mod helloworld {
    tonic::include_proto!("helloworld");
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 {
        println!("Usage: {} connections concurrent_streams_per_conn requests_per_concurrent", args[0]);
        return;
    }
    let connections: usize = args[1].parse().unwrap();
    let concurrent_streams_per_conn: usize = args[2].parse().unwrap();
    let requests_per_concurrent: usize = args[3].parse().unwrap();

    let mut tasks = Vec::new();

    for _ in 0..connections {
        let client = GreeterClient::connect("http://127.0.0.1:50051").await.unwrap();

        for _ in 0..concurrent_streams_per_conn {

            let mut c = client.clone();
            let task = tokio::spawn(async move {

                let now = Instant::now();
                for _ in 0..requests_per_concurrent {
                    let request = tonic::Request::new(HelloRequest {
                        name: "TonicTonic".into(),
                    });
                    let _response = c.say_hello(request).await.unwrap();
                    //println!("RESPONSE={response:?}");
                }
                now.elapsed()
            });
            tasks.push(task);
        }
    }

    let mut first_finish = None;
    let mut last_finish = None;
    for task in tasks.into_iter() {
        let d = task.await.unwrap();
        if first_finish.is_none() {
            first_finish = Some(d);
        }
        last_finish = Some(d);
    }
    println!("done: {:?} ~ {:?}", first_finish.unwrap(), last_finish.unwrap());
}
