# kumo Benchmarks

Head-to-head comparison of kumo, Scrapy, and Colly scraping all 1 000 books from
[books.toscrape.com](https://books.toscrape.com) — 50 pages, same concurrency (16),
median of 3 runs.

## Results — Real Site

Network I/O dominates here; this measures end-to-end throughput over the wire.

| Framework | Language | Time (s) | Items/s | Peak RSS |
|-----------|----------|--------:|--------:|---------:|
| **kumo** | Rust | **14.6** | **68.4** | **14.2 MB** |
| Colly | Go | 14.5 | 68.8 | 33.6 MB |
| Scrapy | Python | 19.5 | 51.4 | 77.1 MB |

- kumo uses **2.4× less memory** than Colly and **5.4× less** than Scrapy
- kumo and Colly reach the same network ceiling; Scrapy trails by ~25%

## Results — Local Mock Server

Network removed; this measures raw framework throughput (parsing, routing, concurrency).

| Framework | Language | Time (s) | Items/s | Peak RSS |
|-----------|----------|--------:|--------:|---------:|
| **kumo** | Rust | **0.08** | **12 346** | **11.5 MB** |
| Colly | Go | 0.23 | 4 310 | 14.3 MB |
| Scrapy | Python | 5.59 | 179 | 69.7 MB |

- kumo is **2.9× faster** than Colly and **69× faster** than Scrapy at raw parsing throughput
- kumo's memory advantage grows: **1.2× over Colly**, **6× over Scrapy**

## Hardware

- **CPU:** Intel Core i7-9750H @ 2.60 GHz (6 cores / 12 threads)
- **RAM:** 16 GB
- **OS:** Windows 11 Home — Docker Desktop (WSL2 backend)
- **Network:** bare metal, residential broadband (real-site runs)

## Methodology

| Parameter | Value |
|-----------|-------|
| Target | `books.toscrape.com` — 1 000 books, 50 pages |
| Concurrency | 16 parallel requests |
| Rate limiting | None |
| robots.txt | Ignored |
| Runs | 3 per framework; results are the **median** |
| Metric | Wall-clock time from process start to last item written |
| Memory | Peak RSS (`VmHWM` from `/proc/self/status`) |

The local mock server is nginx serving pre-generated static HTML with identical
structure to books.toscrape.com — same CSS selectors, same pagination pattern,
instant responses.

## Reproduce

Requirements: Docker, Docker Compose, Python 3.

```bash
cd benchmark

# Real site (3 runs, median)
./run.sh

# Local mock server (eliminates network noise)
./run.sh --local

# Custom number of runs
./run.sh --runs=5
./run.sh --local --runs=5
```

Results are saved to `results/latest.json` (real) and `results/latest_local.json` (local).

## Implementations

| Directory | Language | Version |
|-----------|----------|---------|
| `kumo/` | Rust | latest stable |
| `scrapy/` | Python | 3.12 / Scrapy 2.12 |
| `colly/` | Go | 1.22 / Colly v2 |
| `mockserver/` | nginx | alpine |
