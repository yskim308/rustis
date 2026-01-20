# Rustis

A key-value, in-memory database server written in Rust. The goal is to beat Redis on throughput by leveraging multi-threading and Rust's 'fearless concurrency'. 

Currently, the sever is running single-threaded (same as Redis). The plan is to move to a per-core shared-nothing multi-threaded architecture. Theoretically should get much more throughput; esssentially an architectural copy of more modern alternatives like Dragonfly.

## Benchmark Test Suite

in `benchmark.py` ther are there are four tests 

1. sanity check, just making sure the server works 

2. regular, baseline load (not much stress on the server)

3. High concurrency and throughput with 2000 clients, 32 pipelined requests, and 1 million requests

4. Same as test 3 but with heavy payloads (4KB) 

run these tests with a python runtime (I suggest uv and `uv run benchmark.py`)

> [!NOTE]
> You may have to run `ulimit -n 10000` to allow 2000 concurrent clients!

---

## Official Redis Benchmarks (Baseline)

| Test Suite | Command | Throughput (Req/sec) | Latency p50 (ms) |
| :--- | :--- | :--- | :--- |
| **2. Baseline Load** | `SET` | 239,234.44 | 0.111 |
| *(50 clients, no pipeline)* | `GET` | 244,200.25 | 0.111 |
| | | | |
| **3. High Concurrency** | `RPOP` | **3,690,037.00** | 13.135 |
| *(2k clients, P=32)* | `GET` | 3,355,704.50 | 15.087 |
| | `SET` | 2,710,027.25 | 19.903 |
| | `LPUSH` | 2,597,402.75 | 21.727 |
| | `LPOP` | 2,544,529.25 | 21.999 |
| | | | |
| **4. Heavy Payloads** | `GET` | 794,912.56 | 13.935 |
| *(4KB Data, P=16)* | `SET` | 788,643.50 | 5.471 |

