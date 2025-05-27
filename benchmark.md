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
- GRPC_SERVER_CPUS=1
- GRPC_SERVER_RAM=512m
- GRPC_CLIENT_CONNECTIONS=$C (see below)
- GRPC_CLIENT_CONCURRENCY=$S (see below)
- GRPC_CLIENT_QPS=0
- GRPC_CLIENT_CPUS=12
- GRPC_REQUEST_SCENARIO=complex_proto
- GRPC_GHZ_TAG=0.114.0
```

# Results

```
------------------------------------------------------------------------------------------------------------
| name               |  req/s | avg. cpu | avg. latency |   90 % in |   95 % in |   99 % in |  avg. memory |
--C=1:-------------------------------------------------------------------------------------------------------
----S=1--------^2.9-----------------------------------------------------------------------------------------
| rust_pajamax       |   8487 |   14.25% |      0.10 ms |   0.13 ms |   0.14 ms |   0.17 ms |    496.0 MiB |
| rust_tonic_mt      |   7282 |   35.97% |      0.12 ms |   0.15 ms |   0.16 ms |   0.19 ms |     1.07 MiB |
----S=10-------^4.2-----------------------------------------------------------------------------------------
| rust_pajamax       |  43420 |   25.27% |      0.20 ms |   0.30 ms |   0.33 ms |   0.41 ms |    508.0 MiB |
| rust_tonic_mt      |  40087 |   97.54% |      0.23 ms |   0.29 ms |   0.31 ms |   0.38 ms |     1.17 MiB |
----S=100------^7.0-----------------------------------------------------------------------------------------
| rust_pajamax       | 139524 |   41.78% |      0.67 ms |   0.85 ms |   1.07 ms |   1.66 ms |   613.33 MiB |
| rust_tonic_mt      |  49721 |   104.1% |      1.98 ms |   2.24 ms |   2.31 ms |   2.73 ms |     1.79 MiB |
----S=1000-----^10.4----------------------------------------------------------------------------------------
| rust_pajamax       |  48265 |   10.49% |      1.38 ms |  10.16 ms |  12.79 ms |  22.11 ms |   634.67 MiB |
| rust_tonic_mt      |  47286 |  106.47% |     21.08 ms | 128.25 ms | 149.66 ms | 164.35 ms |     5.97 MiB |
--C=10:-----------------------------------------------------------------------------------------------------
----S=10-------^1.3-----------------------------------------------------------------------------------------
| rust_pajamax       |  34816 |  108.18% |      0.25 ms |   0.27 ms |   0.31 ms |   0.48 ms |     1.18 MiB |
| rust_tonic_mt      |  25764 |  104.15% |      0.37 ms |   0.44 ms |   0.46 ms |   0.52 ms |      1.5 MiB |
----S=100------^2.9-----------------------------------------------------------------------------------------
| rust_pajamax       | 124261 |   86.16% |      0.66 ms |   1.09 ms |   1.37 ms |   2.45 ms |     1.41 MiB |
| rust_tonic_mt      |  52982 |  105.19% |      1.85 ms |   2.64 ms |   2.80 ms |   3.21 ms |     2.82 MiB |
----S=1000-----^7.1-----------------------------------------------------------------------------------------
| rust_pajamax       | 183167 |   43.55% |      3.90 ms |   6.91 ms |   9.00 ms |  13.62 ms |     1.66 MiB |
| rust_tonic_mt      |  62969 |  105.96% |     15.74 ms |  60.15 ms |  95.24 ms | 148.14 ms |    12.07 MiB |
----S=10000----^10.8----------------------------------------------------------------------------------------
| rust_pajamax       |  67335 |   12.78% |     23.03 ms | 252.79 ms | 393.87 ms | 923.98 ms |     1.68 MiB |
| rust_tonic_mt      |  50396 |  103.78% |    195.71 ms | 865.93 ms |    1.13 s |    2.08 s |     78.8 MiB |
--C=100:----------------------------------------------------------------------------------------------------
----S=100------^1.8-----------------------------------------------------------------------------------------
| rust_pajamax       |  42563 |  104.01% |      2.16 ms |   1.68 ms |   2.38 ms |  54.81 ms |     7.87 MiB |
| rust_tonic_mt      |  24234 |  104.45% |      4.10 ms |   4.49 ms |   4.57 ms |   4.78 ms |     5.84 MiB |
----S=1000-----^3.2-----------------------------------------------------------------------------------------
| rust_pajamax       | 144842 |    98.6% |      5.25 ms |   9.80 ms |  14.04 ms |  25.94 ms |     9.03 MiB |
| rust_tonic_mt      |  48014 |   104.5% |     20.70 ms |  74.04 ms |  90.44 ms | 144.46 ms |    14.62 MiB |
----S=10000----^6.7-----------------------------------------------------------------------------------------
| rust_pajamax       | 130633 |   34.75% |     62.12 ms | 130.53 ms | 204.77 ms | 339.16 ms |    10.17 MiB |
| rust_tonic_mt      |  57798 |  103.47% |    170.28 ms | 846.56 ms | 956.41 ms |    1.73 s |    77.27 MiB |
----S=100000---^11.0---------------------------------------------------------------------------------------
| rust_pajamax       |  85893 |   16.78% |    273.69 ms | 618.88 ms | 733.04 ms |    1.06 s |     8.96 MiB |
| rust_tonic_mt      |  39148 |   84.15% |       2.20 s |    5.42 s |    7.15 s |   12.25 s |   465.75 MiB |
------------------------------------------------------------------------------------------------------------
```

In most cases, there is a significant difference in CPU usage between Pajamax
and Tonic. Therefore, what we are comparing here is `req/s` per `CPU`.
The `^*` marked in the figure above represents the ratio of req/s per CPU
between Pajamax and Tonic. It can be considered as how is Pajamax faster
than Tonic.


# Conclusion

As can be seen from the figure above, under the same `C`
(concurrent connections), the higher the value of `S` (concurrent streams),
the greater the performance advantage of Pajamax over Tonic. When `S/C` = 1000,
that is, when there are on average 1000 concurrent streams per connection,
Pajamax is abount 10X faster than Tonic.

I guess that this is because the most performance cost for Pajamax
lies in network I/O system calls. As concurrency increases, the number
of streams arriving simultaneously on each connection grows. Consequently,
each system call handles more requests, reducing the frequency of
system calls and thereby enhancing performance.
