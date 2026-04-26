# kumo Benchmarks

Head-to-head comparison of kumo, Scrapy, and Colly scraping all 1 000 books from
[books.toscrape.com](https://books.toscrape.com) — 50 pages, same concurrency (16),
median of 3 runs.

## Results — Real Site

Network I/O dominates here; this measures end-to-end throughput over the wire.

| Framework | Language | Time (s) | Items/s | Peak RSS |
|-----------|----------|--------:|--------:|---------:|
| **kumo** | Rust | **13.0** | **76.7** | **12.5 MB** |
| Colly | Go | 13.6 | 73.5 | 31.4 MB |
| Scrapy | Python | 18.7 | 53.3 | 77.2 MB |

- kumo uses **2.5× less memory** than Colly and **6.2× less** than Scrapy
- kumo is **4% faster** than Colly; Scrapy trails by ~31%

## Results — Local Mock Server

Network removed; this measures raw framework throughput (parsing, routing, concurrency).

| Framework | Language | Time (s) | Items/s | Peak RSS |
|-----------|----------|--------:|--------:|---------:|
| **kumo** | Rust | **0.08** | **12 346** | **11.3 MB** |
| Colly | Go | 0.24 | 4 098 | 15.8 MB |
| Scrapy | Python | 5.57 | 180 | 69.9 MB |

- kumo is **3.0× faster** than Colly and **69× faster** than Scrapy at raw parsing throughput
- kumo's memory advantage: **1.4× over Colly**, **6.2× over Scrapy**

## Scaling Results — Local Mock Server

How throughput (items/s) scales with concurrency. 1 run per level, local mock server:

| Concurrency | **kumo** | Colly (Go) | Scrapy (Python) |
|------------:|--------:|-----------:|----------------:|
| 16 | **4 831** | 3 937 | 181 |
| 32 | **11 765** | 4 608 | 177 |
| 64 | **12 048** | 4 237 | 181 |
| 128 | **12 987** | 3 891 | 181 |

- kumo scales **2.7× from 16→32** then plateaus near nginx's static-file serving ceiling (~13 000 items/s)
- Colly plateaus at ~4 000–4 600 items/s regardless of concurrency — goroutine scheduling overhead limits further gain
- Scrapy is flat at ~180 items/s across all levels — the Python GIL prevents true parallel I/O beyond a narrow window

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
