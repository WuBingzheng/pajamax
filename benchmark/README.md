Benchmark for Pajamax vs Tonic

# Programs

There are 3 programs:

- pajamax server
- tonic server
- benchmark client

## pajamax server

Use the helloworld example.

The number of threads is determined by the client's concurrent connections.

Run it:

```
$ cd examples/
$ cargo run --release --bin helloworld
```

## tonic server

Use tonic's helloworld example.

There are 1 argument:

1. number of threads.

Run it:

```
$ cd benchmark/
$ cargo run --release --bin tonic-helloworld-server 4
```

## benchmark client

There are 4 arguments:

1. server's address
2. number of concurrent connections,
3. number of concurrent streams per connection,
4. number of requests per concurrent stream.

Run it:

```
$ cd benchmark/
$ cargo run --release --bin bench-client 'http://127.0.0.1:50051' 4 100 10000
```

# My Test Result

Machines:

- 1 machine for both Pajamax and Tonic servers, with CPU: Intel(R) Xeon(R) Platinum 8259CL CPU @ 2.50GHz
- 2 machines for benchmark client, both with CPU: Intel(R) Xeon(R) Platinum 8124M CPU @ 3.00GHz

Test cases:

- (A): 100 concurrent streams per connection, and 10000 requests per concurrent stream.
- (B): 1000 concurrent streams per connection, and 1000 requests per concurrent stream.

Results:

```
#thread |  time(s)  |    cpu%    |  total cpu%
        |           | per thread |  of clients
-(A)----+-----------+------------+-------------
   2    |  36   33  |    9   87  |  1600  1400
   4    |  35   38  |    9   85  |  1200  1100
   8    |  34   44  |    8   83  |  1800  1700
-(B)----+-----------+------------+-------------
   2    |  23   27  |   10   90  |  900   800
   4    |  23   33  |    9   87  |  1600  1400
   8    |  20   44  |    3   83  |  2200  1700
```

Explaination:

- #thread: number of server threads. This is determined by client's
  concurrent connection for Pajamax server, and by argument for Tonic
  server. Since there are 2 clients, so we can not test 1 thread
  Pajamax server.

- time(s): client run time in seconds. Pajamax vs Tonic. The data shows
  that Pajamax is a little faster.

- cpu% per thread: server's cpu% for each thread. Pajamax vs Tonic.
  *The difference is big*. Pajamax's CPU is 9X ~ 10X free than Tonic's.
  Here I can not fully utilize the Pajamax's CPU. It may be need
  faster bench client or more machines.

- total cpu% of clients. Pajamax vs Tonic.

Conclusion:

It can be seen that Pajamax is a bit faster than Tonic in terms of test
time, and has a much 9X ~ 10X lower CPU idle rate. I think it can be
roughly concluded that Pajamax is 10X faster than Tonic.
