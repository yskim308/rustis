# Rustis

A key-value, in-memory database server written in Rust. The goal is to beat Redis on throughput by leveraging multi-threading and Rust's 'fearless concurrency'. 

Currently, the sever is running single-threaded (same as Redis). The plan is to move to a per-core shared-nothing multi-threaded architecture. Theoretically should get much more throughput; esssentially an architectural copy of more modern alternatives like Dragonfly.

## Quick Start

Install `redis` with any package manager of choice then run

```bash
cargo run --release

```
and in another terminal window, run the benchmark or `redis-cli` to test

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

## Supported Commands

Currently the following commands are supported: 

- Basic: `GET`, `SET`

- List: `LPUSH`, `RPUSH`, `RPOP`, `LPOP`, `LRANGE`

- Set: `SADD`, `SPOP`, `SMEMBERS`

---

# Current Benchmarks

## Redis Baseline (official redis-server benchmarks)

|Test Name                            |Command|RPS       |Latency (p50)|
|-------------------------------------|-------|----------|-------------|
|Regular Load (Baseline)              |SET    |236686.38 |0.111        |
|Regular Load (Baseline)              |GET    |245700.25 |0.111        |
|High Concurrency & Throughput (Mixed)|SET    |874890.62 |76.351       |
|High Concurrency & Throughput (Mixed)|GET    |2857143.00|18.351       |
|High Concurrency & Throughput (Mixed)|LPUSH  |2525252.50|21.615       |
|High Concurrency & Throughput (Mixed)|LPOP   |2450980.50|22.367       |
|Heavy Payload Saturation (4KB)       |SET    |480769.25 |9.919        |
|Heavy Payload Saturation (4KB)       |GET    |618811.88 |19.535       |

---

## single_thread_v3

1. conditional compaction of Bytes, use references if payload is large

2. increase read and write buffer size, avoid thrashing from malloc 

### single_thread_v3 vs single_thread_v2


| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| High Concurrency & Throughput (Mixed) | SET | 1,225,490 | 🟢 +0.12% | 43.839 | 🟢 -5.39% |
| High Concurrency & Throughput (Mixed) | GET | 2,450,980 | 🟢 +12.50% | 23.023 | 🟢 -8.69% |
| High Concurrency & Throughput (Mixed) | LPUSH | 2,242,152 | 🟢 +2.47% | 26.015 | 🟢 -3.67% |
| High Concurrency & Throughput (Mixed) | LPOP | 2,762,431 | 🟢 +1.10% | 20.671 | 🟢 -2.34% |
| High Concurrency & Throughput (Mixed) | SADD | 1,890,359 | 🟢 +1.89% | 31.519 | 🟢 -3.57% |
| High Concurrency & Throughput (Mixed) | SPOP | 1,742,160 | 🟢 +0.87% | 16.639 | 🟢 -2.53% |
| Heavy Payload Saturation (4KB) | SET | 452,489 | 🟢 +27.69% | 33.855 | 🟢 -21.80% |
| Heavy Payload Saturation (4KB) | GET | 792,393 | 🟢 +16.80% | 18.175 | 🟢 -8.24% |


### single_thread_v3 vs redis baseline


| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| High Concurrency & Throughput (Mixed) | SET | 1,225,490 | 🟢 +38.73% | 43.839 | 🟢 -42.58% |
| High Concurrency & Throughput (Mixed) | GET | 2,450,980 | 🔴 -14.46% | 23.023 | 🔴 +26.12% |
| High Concurrency & Throughput (Mixed) | LPUSH | 2,242,152 | 🔴 -13.00% | 26.015 | 🔴 +23.00% |
| High Concurrency & Throughput (Mixed) | LPOP | 2,762,431 | 🟢 +13.54% | 20.671 | 🟢 -9.27% |
| High Concurrency & Throughput (Mixed) | SADD | 1,890,359 | 🔴 -27.60% | 31.519 | 🔴 +54.15% |
| High Concurrency & Throughput (Mixed) | SPOP | 1,742,160 | 🔴 -45.99% | 16.639 | 🔴 +25.76% |
| Heavy Payload Saturation (4KB) | SET | 452,489 | 🔴 -7.24% | 33.855 | 🔴 +248.05% |
| Heavy Payload Saturation (4KB) | GET | 792,393 | 🟢 +31.22% | 18.175 | 🟢 -12.82% |

---

# Benchmarks for Previous Versions

## single_thread_v2

1. remove atomic reference counting in hash map

    - to do this, turn hash map to simple reference counted cell

    - main loop now runs with Tokio's LocalSet, gauranteeing no sharing between threads

2. Use Bytes instead of owned Keys in hash map 

3. Compact Bytes values to avoid the heap from holding onto buffer space 

### single_thread_v2 vs single_thread_v1


| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Regular Load (Baseline) | SET | 234,742 | 🔴 -0.70% | 0.111 | 🟢 0.00% |
| Regular Load (Baseline) | GET | 243,309 | 🔴 -0.12% | 0.111 | 🟢 0.00% |
| High Concurrency & Throughput (Mixed) | SET | 1,223,990 | 🟢 +20.81% | 46.335 | 🟢 -20.00% |
| High Concurrency & Throughput (Mixed) | GET | 2,178,649 | 🟢 +12.85% | 25.215 | 🟢 -9.37% |
| High Concurrency & Throughput (Mixed) | LPUSH | 2,188,184 | 🔴 -9.63% | 27.007 | 🔴 +12.16% |
| High Concurrency & Throughput (Mixed) | LPOP | 2,732,240 | 🟢 +3.01% | 21.167 | 🟢 -4.48% |
| High Concurrency & Throughput (Mixed) | SADD | 1,855,288 | 🔴 -2.41% | 32.687 | 🔴 +3.39% |
| High Concurrency & Throughput (Mixed) | SPOP | 1,727,116 | 🟢 +0.52% | 17.071 | 🟢 -3.61% |
| Heavy Payload Saturation (4KB) | SET | 354,359 | 🔴 -0.14% | 43.295 | 🟢 -0.07% |
| Heavy Payload Saturation (4KB) | GET | 678,426 | 🔴 -11.13% | 19.807 | 🔴 +6.72% |

### single_thread_v2 vs base redis


| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Regular Load (Baseline) | SET | 234,742 | 🔴 -2.00% | 0.111 | 🟢 0.00% |
| Regular Load (Baseline) | GET | 243,309 | 🔴 -0.61% | 0.111 | 🟢 0.00% |
| High Concurrency & Throughput (Mixed) | SET | 1,223,990 | 🟢 +38.56% | 46.335 | 🟢 -39.31% |
| High Concurrency & Throughput (Mixed) | GET | 2,178,649 | 🔴 -23.97% | 25.215 | 🔴 +38.13% |
| High Concurrency & Throughput (Mixed) | LPUSH | 2,188,184 | 🔴 -15.10% | 27.007 | 🔴 +27.69% |
| High Concurrency & Throughput (Mixed) | LPOP | 2,732,240 | 🟢 +12.30% | 21.167 | 🟢 -7.09% |
| High Concurrency & Throughput (Mixed) | SADD | 1,855,288 | 🔴 -28.94% | 32.687 | 🔴 +59.86% |
| High Concurrency & Throughput (Mixed) | SPOP | 1,727,116 | 🔴 -46.46% | 17.071 | 🔴 +29.02% |
| Heavy Payload Saturation (4KB) | SET | 354,359 | 🔴 -27.36% | 43.295 | 🔴 +345.10% |
| Heavy Payload Saturation (4KB) | GET | 678,426 | 🟢 +12.35% | 19.807 | 🟢 -4.99% |

---

## single_thread_v1

1. Move from `vec<u8>` to `Bytes`, zero copy allocations

2. use `memchr` crate 

3. refactor to use `BytesMut`

4. use `--release` on cargo compile

### Results Compared to Unoptimized


| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Regular Load (Baseline) | SET | 236,407 | 🟢 +45.27% | 0.111 | 🟢 -63.37% |
| Regular Load (Baseline) | GET | 243,605 | 🟢 +33.13% | 0.111 | 🟢 -59.04% |
| High Concurrency & Throughput (Mixed) | SET | 1,013,171 | 🟢 +240.22% | 57.919 | 🟢 -72.31% |
| High Concurrency & Throughput (Mixed) | GET | 1,930,502 | 🟢 +439.00% | 27.823 | 🟢 -84.13% |
| High Concurrency & Throughput (Mixed) | LPUSH | 2,421,308 | 🟢 +588.38% | 24.079 | 🟢 -86.51% |
| High Concurrency & Throughput (Mixed) | LPOP | 2,652,520 | 🟢 +573.21% | 22.159 | 🟢 -86.29% |
| High Concurrency & Throughput (Mixed) | SADD | 1,901,141 | 🟢 +533.27% | 31.615 | 🟢 -84.94% |
| High Concurrency & Throughput (Mixed) | SPOP | 1,718,213 | 🟢 +3584.19% | 17.711 | 🟢 -88.24% |
| Heavy Payload Saturation (4KB) | SET | 354,862 | 🟢 +133.22% | 43.327 | 🟢 -58.31% |
| Heavy Payload Saturation (4KB) | GET | 763,359 | 🟢 +187.33% | 18.559 | 🟢 -68.51% |


### Results Compared to Redis Base 


| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
| Regular Load (Baseline) | SET | 236,407 | 🔴 -1.30% | 0.111 | 🟢 0.00% |
| Regular Load (Baseline) | GET | 243,605 | 🔴 -0.49% | 0.111 | 🟢 0.00% |
| High Concurrency & Throughput (Mixed) | SET | 1,013,171 | 🟢 +14.69% | 57.919 | 🟢 -24.14% |
| High Concurrency & Throughput (Mixed) | GET | 1,930,502 | 🔴 -32.63% | 27.823 | 🔴 +52.41% |
| High Concurrency & Throughput (Mixed) | LPUSH | 2,421,308 | 🔴 -6.05% | 24.079 | 🔴 +13.84% |
| High Concurrency & Throughput (Mixed) | LPOP | 2,652,520 | 🟢 +9.02% | 22.159 | 🟢 -2.74% |
| High Concurrency & Throughput (Mixed) | SADD | 1,901,141 | 🔴 -27.19% | 31.615 | 🔴 +54.62% |
| High Concurrency & Throughput (Mixed) | SPOP | 1,718,213 | 🔴 -46.74% | 17.711 | 🔴 +33.86% |
| Heavy Payload Saturation (4KB) | SET | 354,862 | 🔴 -27.25% | 43.327 | 🔴 +345.43% |
| Heavy Payload Saturation (4KB) | GET | 763,359 | 🟢 +26.41% | 18.559 | 🟢 -10.98% |


---
## unoptimized_v1 vs Redis Base

first iteration baseline vs the official redis-server

| Test Name | Cmd | RPS | Δ RPS | Latency (ms) | Δ Lat |
| :--- | :--- | :--- | :--- | :--- | :--- |
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

