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

Running `benchmark.py` will give you the an option to save to a csv. If you wish to benchmark your own, delete the existing csv file. 

Running `generate_report.py` will give you an option to print out a table comparing different test runs


--- 

## Current Benchmarks

### Redis Baseline (official redis-server benchmarks)**

|Test Name                            |Command|RPS       |Latency (p50)|
|-------------------------------------|-------|----------|-------------|
|Quick Sanity Check                   |INLINE |124999.99 |0.207        |
|Quick Sanity Check                   |MBULK  |83333.34  |0.543        |
|Regular Load (Baseline)              |SET    |236686.38 |0.111        |
|Regular Load (Baseline)              |GET    |245700.25 |0.111        |
|High Concurrency & Throughput (Mixed)|SET    |874890.62 |76.351       |
|High Concurrency & Throughput (Mixed)|GET    |2857143.00|18.351       |
|High Concurrency & Throughput (Mixed)|LPUSH  |2525252.50|21.615       |
|High Concurrency & Throughput (Mixed)|LPOP   |2450980.50|22.367       |
|Heavy Payload Saturation (4KB)       |SET    |480769.25 |9.919        |
|Heavy Payload Saturation (4KB)       |GET    |618811.88 |19.535       |

### Current Rustis Implementation Benchmark
| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Quick Sanity Check | INLINE | 76,923 | 🔴 -46.15% | 0.591 | 🔴 +209.42% |
| Quick Sanity Check | MBULK | 90,909 | 🟢 +9.09% | 0.591 | 🟢 -6.34% |
| Regular Load (Baseline) | SET | 162,734 | 🔴 -32.06% | 0.303 | 🔴 +172.97% |
| Regular Load (Baseline) | GET | 182,983 | 🔴 -25.25% | 0.271 | 🔴 +144.14% |
| High Concurrency & Throughput (Mixed) | SET | 297,796 | 🔴 -66.29% | 209.151 | 🔴 +173.93% |
| High Concurrency & Throughput (Mixed) | GET | 358,166 | 🔴 -87.50% | 175.359 | 🔴 +860.61% |
| High Concurrency & Throughput (Mixed) | LPUSH | 351,741 | 🔴 -86.35% | 178.559 | 🔴 +744.21% |
| High Concurrency & Throughput (Mixed) | LPOP | 394,011 | 🔴 -83.81% | 161.663 | 🔴 +609.58% |
| High Concurrency & Throughput (Mixed) | SADD | 300,210 | 🔴 -88.50% | 209.919 | 🔴 +926.65% |
| High Concurrency & Throughput (Mixed) | SPOP | 46,637 | 🔴 -98.55% | 150.655 | 🔴 +1038.65% |
| Heavy Payload Saturation (4KB) | SET | 152,161 | 🔴 -68.81% | 103.935 | 🔴 +968.52% |
| Heavy Payload Saturation (4KB) | GET | 265,675 | 🔴 -56.00% | 58.943 | 🔴 +182.74% |

*LOTS* of work to be done...
