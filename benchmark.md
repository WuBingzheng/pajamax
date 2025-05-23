We use [grpc-bench](https://github.com/WuBingzheng/grpc_bench/tree/add-rust_pajamax_bench)
project to compare the performance between Pajamax and Tonic.

# Machine Specs

```
OS: Ubuntu 22.04.5 LTS
Kernel: Linux 5.15.0-125-generic
CPU: AMD EPYC 9754 128-Core Processor, 16 CPUs
Memory: 32 GiB
```

# Parameters

```
- GRPC_BENCHMARK_DURATION=20s
- GRPC_BENCHMARK_WARMUP=5s
- GRPC_SERVER_CPUS=$CPU (see below)
- GRPC_SERVER_RAM=512m
- GRPC_CLIENT_CONNECTIONS=$CONN (see below)
- GRPC_CLIENT_CONCURRENCY=1000
- GRPC_CLIENT_QPS=0
- GRPC_CLIENT_CPUS=12
- GRPC_REQUEST_SCENARIO=complex_proto
- GRPC_GHZ_TAG=0.114.0
```

# Results

```
--------------------------------------------------------------------------------------------------------  ---------------
| name            |  req/s | avg. latency |   90 % in |   95 % in |   99 % in | avg. cpu | avg. memory |   req/s / cpu% |
--------------------------------------------------------------------------------------------------------  ---------------
-CPU=1, CONN=1------------------------------------------------------------------------------------------  ---------------
| rust_pajamax    |  47311 |      1.30 ms |   8.70 ms |  11.04 ms |  23.79 ms |   10.39% |  573.33 MiB |     455351     |
| rust_tonic_mt   |  46641 |     21.36 ms | 129.24 ms | 151.99 ms | 166.64 ms |  104.44% |     5.9 MiB |      44658     |
------- CONN=5------------------------------------------------------------------------------------------  ---------------
| rust_pajamax    | 184744 |      3.70 ms |   7.09 ms |   9.22 ms |  14.44 ms |   48.96% |    1.39 MiB |     377337     |
| rust_tonic_mt   |  58727 |     16.88 ms |  67.88 ms | 103.14 ms | 159.81 ms |  104.15% |   10.98 MiB |      56386     |
------- CONN=50-----------------------------------------------------------------------------------------  ---------------
| rust_pajamax    | 161600 |      4.73 ms |   8.73 ms |  11.65 ms |  19.71 ms |   76.18% |    5.06 MiB |     212129     |
| rust_tonic_mt   |  58101 |     17.10 ms |  65.95 ms |  89.59 ms | 141.64 ms |  102.57% |   13.36 MiB |      56645     |
--------------------------------------------------------------------------------------------------------  ---------------
-CPU=4, CONN=4------------------------------------------------------------------------------------------  ---------------
| rust_pajamax    | 180144 |      3.94 ms |   7.96 ms |  10.15 ms |  14.98 ms |    41.0% |    1.32 MiB |     439376     |
| rust_tonic_mt   | 124891 |      7.04 ms |  11.21 ms |  13.15 ms |  17.23 ms |  258.38% |   19.86 MiB |      48336     |
------- CONN=20-----------------------------------------------------------------------------------------  ---------------
| rust_pajamax    | 172577 |      4.27 ms |   7.56 ms |  10.09 ms |  16.69 ms |   59.21% |    2.54 MiB |     291466     |
| rust_tonic_mt   | 123319 |      6.94 ms |  12.00 ms |  14.68 ms |  21.03 ms |  288.03% |   17.83 MiB |      42814     |
------- CONN=200----------------------------------------------------------------------------------------  ---------------
| rust_pajamax    | 128005 |      5.96 ms |  10.73 ms |  15.80 ms |  33.18 ms |  130.38% |   16.48 MiB |      98178     |
| rust_tonic_mt   |  95500 |      9.01 ms |  16.34 ms |  21.16 ms |  35.64 ms |  305.25% |   23.57 MiB |      31285     |
--------------------------------------------------------------------------------------------------------  ---------------
```

In most test cases, the CPU of Pajamax was not fully utilized. This is likely
due to the performance of the client `ghz` can not keep up with Pajamax even
with more CPUs. To better compare performance with Tonic, I added a column
to the results, which shows the ratio of column `req/s` to `avg. cpu`.


# Conclusion

From the results above, it can be seen that the performance of Pajamax and
Tonic is very different.

When the number of CPUs is the same, the req/s and CPU usage of Tonic do
not change much for different client connection numbers, but the changes
for Pajamax are very significant. This is because Tonic is based on `tokio`,
which will start a fixed number (here is `GRPC_SERVER_CPUS`) of threads
to handle requests. In contrast, Pajamax creates a thread for each
connection, and the number of threads affects performance.

When the number of connections is the same as the number of CPUs, the
CPU utilization is very low because the client `ghz` cannot fully load
Pajamax, but the relative utilization (the last column in the table above)
is the highest. As the number of connections increases, the number of
threads also increases, and the CPU utilization becomes higher. However,
due to the performance overhead caused by thread switching, the
relative utilization decreases.

If we only look at the last column, Pajamax is up to 10 times faster than
Tonic for few connections.
