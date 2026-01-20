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
| Quick Sanity Check | INLINE | 83,333 | 🔴 -33.33% | 0.583 | 🔴 +181.64% |
| Quick Sanity Check | MBULK | 71,429 | 🔴 -14.29% | 0.655 | 🔴 +20.63% |
| Regular Load (Baseline) | SET | 163,132 | 🔴 -31.08% | 0.303 | 🔴 +172.97% |
| Regular Load (Baseline) | GET | 181,984 | 🔴 -25.93% | 0.271 | 🔴 +144.14% |
| High Concurrency & Throughput (Mixed) | SET | 281,452 | 🔴 -67.83% | 228.991 | 🔴 +199.92% |
| High Concurrency & Throughput (Mixed) | GET | 370,370 | 🔴 -87.04% | 169.983 | 🔴 +826.29% |
| High Concurrency & Throughput (Mixed) | LPUSH | 353,982 | 🔴 -85.98% | 179.327 | 🔴 +729.64% |
| High Concurrency & Throughput (Mixed) | LPOP | 375,940 | 🔴 -84.66% | 161.919 | 🔴 +623.92% |
| Heavy Payload Saturation (4KB) | SET | 152,068 | 🔴 -68.37% | 103.743 | 🔴 +945.90% |
| Heavy Payload Saturation (4KB) | GET | 244,499 | 🔴 -60.49% | 58.879 | 🔴 +201.40% |

*LOTS* of work to be done...
