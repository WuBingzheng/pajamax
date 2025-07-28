Here we use [`ghz`](https://ghz.sh/) directly to benchmark and compare
Tonic and Pajamax, and look into the frame graphs.

# Directions

CPU: AMD EPYC 9754 128-Core Processor, 16 cores

Tonic server: Tonic's [`helloworld` example](tonic-helloworld/src/main.rs)
with the changing to specify the number of tokio workers.
We only test 1 worker case here.

Pajamx server: Pajamax's [`helloworld` example](https://github.com/WuBingzheng/pajamax/blob/main/examples/src/helloworld.rs).

Bench client `ghz` command line:

```
ghz --proto=proto/helloworld.proto --call helloworld.Greeter/SayHello --insecure 127.0.0.1:50051 --cpus 12 -z1m
```

Since the 2 servers are both much faster than client, we set 12 CPUs for
client and 1 CPU for servers.
For the `ghz` client we use `--cpus 12`.
For the Tonic-server, we set 1 tokio worker in the code.
For the Pajamax-server, it creates 1 thread for each connection and
`ghz` uses 1 connection in default.

# Results

The full result: [Tonic](./tonic.ghz.out) and [Pajamax](./pajamax.ghz.out).

The summary:

```
        | client | server |    r/s
        |  CPU % |  CPU % |
--------+--------+--------+-----------
Tonic   |   790% |    95% |  41124.74
Pajamax |  1100% |    15% |  65135.31
```

We can say that Pajamax is `(65135.31/15%) / (41124.74/95%)` = 9.5X faster that Tonic.

# Frame Graphs

(You may need to download the SVG file to browse it interactively.)

The [frame graph of Tonic](./tonic.flame.svg):

- `tokio::io::poll_evented::PollEvented<E>::poll_read`, 1.60%, network input, the recv() syscall,
- `<tokio::net::tcp::stream::TcpStream as ...>::poll_write_vectored`, 38.11%, network output, the send() syscall,
- others, mostly tokio runtime and protocol processing (protobuf and http2).

The [frame graph of Pajamax](./pajamax.flame.svg):

- `<helloworld::helloworld::GreeterServer<T> as ...>::handle`, 7.09%, construct the response, mostly the string formating and gRPC encoding (protobuf and http2),
- `pajamax::hpack_decoder::Decoder::find_path`, 2.60%, process the request, mostly the `:path` header,
- `pajamax::response_end::ResponseEnd::flush`, 40.32%, network output, the send() syscall,
- `std::os::unix::net::datagram::UnixDatagram::recv`, 47.55%, network input, the recv() syscall.

Summary:

- I don't know why in frame graph of Tonic, the proportion of network input
  is so small, only 1.60%? In contrast, in Pajamax, it is 47.55%.
  I guess it may be because the network in Tonic is non-blocking, and most
  TCP protocol processing has been completed by kernel in the background,
  and the application only needs to read the processed data from the kernel.
  The processing work is not counted in this process, and is not counted
  in this flame graph.

- In Tonic, the tokio runtime and protocol processing account for the
  majority of the workload, while in Pajamax it only accounts for about 5%.
  This is why Pajamax is so faster than Tonic.

- It can be confirmed that the response string formating (`alloc::fmt::format::format_inner`)
  does the same work in both 2 programs, which accounts 0.31% and 2.90%
  in Tonic and Pajamax respectively.
  The difference between the two is about 9.4 times, which is very close
  to the conclusion of the benchmark above (9.5X).

