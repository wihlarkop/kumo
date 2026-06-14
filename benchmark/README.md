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

The previous scale table used one serial pagination chain, which did not keep
multiple requests runnable and therefore could not measure scheduler scaling.
It has been removed.

The current `scale` workflow is Kumo-specific and uses 64 independent
pagination chains, four pages per chain, for 5,120 items. It measures
concurrency `1`, `4`, `8`, `16`, `32`, and `64`, with three runs per level.

The uploaded `summary.json` and `summary.md` report median throughput, elapsed
time, peak RSS, fetch and parse timings, and peak requests in flight. The
report marks the first level where throughput improves by less than 5% or RSS
grows by more than 25%.

## Large-Crawl Validation

The manual GitHub `Benchmark` workflow provides two Kumo-only correctness and
memory workloads:

| Mode | Pages | Items | Crawl shape |
|---|---:|---:|---|
| `soak` | 500 | 10,000 | 100 independent chains |
| `large` | 5,000 | 100,000 | 100 independent chains |

Both modes fail when the crawler reports the wrong item or page count, the
JSONL output contains missing or duplicate items, or the benchmark process
fails. Reports include throughput, peak RSS, RSS per 1,000 items, first-10k
throughput, and throughput after the first 10,000 items. Memory is reported
without a fixed failure threshold until three successful 100k baselines exist.

The mockserver image accepts `TOTAL_PAGES`, `ITEMS_PER_PAGE`, and
`WORKLOAD_CHAINS` build arguments. The normal comparison workload remains fixed
at 50 pages and 1,000 items.

## Realistic Resilience Benchmark

The manual `realistic` workflow mode runs Kumo against a deterministic
production-like server instead of the zero-latency nginx fixture:

- 200 pages and 4,000 unique items across 20 independent chains
- 20, 40, 80, and 120 ms response-latency classes
- 1, 8, 32, and 128 KiB response-payload classes
- twenty first-attempt HTTP 503 responses
- four first-attempt HTTP 429 responses with `Retry-After: 0`

Kumo uses `StatusRetry` and a bounded `RetryPolicy`. The typed report
cross-checks Kumo stats, JSONL rows, and server counters. It fails unless all
4,000 unique items and 200 pages are recovered with exactly 24 retries, zero
retry exhaustion, zero final errors, and the expected 200/429/503 status
counts.

Throughput on shared GitHub runners is informational. Exact crawl, retry, and
server counters are hard correctness gates.

The `realistic-compare` workflow mode applies the same workload to Kumo,
Scrapy, and Colly three times by default. A seeded framework order is rotated
between runs so each framework occupies each execution position once. The
server is reset before every framework, and every individual run must produce
4,000 unique items, 200 successful pages, 24 retries, zero exhausted retries,
zero final errors, and exact server status counters before median timing and
memory are compared.

Colly runs with asynchronous collection enabled so its configured parallelism
is effective. Scrapy enables its retry middleware only for this benchmark.
The report records the seed, execution order, median throughput and RSS, and
their minimum-to-maximum spread. A seed of zero derives a reproducible value
from the GitHub workflow run ID.

## Output-Store Overhead Benchmark

The manual `store` workflow mode isolates the cost of Kumo's production
`JsonlStore`. It runs the same 100,000-item local workload with JSONL output
and with a benchmark-only no-op store. Each variant runs three times, and the
execution order alternates to reduce shared-runner position bias.

Every sample must report exactly 100,000 items and 5,000 pages with zero crawl
errors and zero exhausted retries. JSONL samples must also contain exactly
100,000 output rows. The report includes median and min-max throughput, elapsed
time, peak RSS, and cumulative store time.

This benchmark is Kumo-only because it diagnoses an internal hot path rather
than comparing framework features. A no-op throughput gain of at least 10%
marks JSONL storage as a material bottleneck. A smaller difference does not
justify adding asynchronous batching and shutdown complexity.

The first CI run measured a 3.2% median throughput difference, so JSONL
batching was not selected as the next optimization. Request URL parsing was
profiled next because fingerprinting, scheduling, robots handling, filtering,
statistics, and events previously parsed the same immutable URL independently.

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

Requirements: Docker and Docker Compose.

```bash
cd benchmark

# Real site (3 runs, median)
./run.sh

# Local mock server (eliminates network noise)
./run.sh --local

# Kumo-only correctness and memory validation
./run.sh --soak --concurrency=8
./run.sh --large --concurrency=8

# Kumo-only variable-latency and retry validation
./run.sh --realistic --concurrency=8

# Correctness-gated, balanced Kumo/Scrapy/Colly comparison
./run.sh --realistic-compare --runs=3 --seed=20260614 --concurrency=8

# Kumo-only JSONL versus no-op output-store comparison
./run.sh --store --concurrency=16

# Custom number of runs
./run.sh --runs=5
./run.sh --local --runs=5
```

On Windows, use the PowerShell runner instead of WSL bash:

```powershell
cd benchmark

# Real site (3 runs, median)
.\run.ps1

# Local mock server (eliminates network noise)
.\run.ps1 -Local

# Custom number of runs
.\run.ps1 -Runs 5
.\run.ps1 -Local -Runs 5
```

Results are saved to `results/latest.json` (real) and `results/latest_local.json` (local).
Each result includes the benchmarked framework, language runtime, library
version, concurrency, elapsed time, item count, and peak RSS.

Kumo results also include a `timings` object from `CrawlReport`, with cumulative
successful-request phase timings for request middleware, fetching, response
middleware, parsing, item pipelines, and storage. These values are diagnostic
signals, not wall-clock percentages: concurrent request phases can overlap, so
their sum can exceed the process elapsed time.

The local mock benchmark is the primary signal for request-dispatch ownership
and allocation changes. Use at least five runs with the same concurrency and
compare medians; changes below 5% on shared runners should be treated as noise.
Confirm accepted hot-path changes with the 100k-item large mode before merging.

## Compare Results

Use the typed Rust comparison tool to compare a trusted baseline with a new
summary:

```bash
cargo run -p kumo-benchmark-compare -- \
  benchmark/baselines/local.json \
  benchmark/results/latest_local.json \
  --output benchmark/results/comparison.md \
  --json-output benchmark/results/comparison.json
```

The report shows percentage changes in elapsed time, throughput, and peak RSS.
When both inputs contain Kumo timing data, it also compares request phase
timings. Lower elapsed time, memory, and phase duration are improvements; higher
throughput is an improvement.

GitHub's manual `Benchmark` workflow runs this comparison after local or
real-site benchmarks, adds the Markdown report to the workflow summary, and
uploads both report formats with the raw results. Comparisons are informational:
shared CI runner noise must not automatically block a merge or release.

For scheduler scaling, dispatch the workflow in `scale` mode. The typed
reporter reads the three raw Kumo results for every concurrency level:

```bash
cargo run -p kumo-benchmark-compare -- scale benchmark/results/scale \
  --output benchmark/results/scale/summary.md \
  --json-output benchmark/results/scale/summary.json
```

For large-crawl validation, dispatch `soak` first and then `large`. The large
mode is manual and has a dedicated 45-minute step timeout. A public 100k-item
claim requires three consecutive successful large runs with exact counts, zero
duplicates, bounded RSS growth, and no sustained throughput collapse after the
first 10,000 items.

For output-store diagnosis, dispatch `store`. The workflow runs all six
samples, validates their correctness, appends the typed report to the workflow
summary, and retains raw stats plus JSONL row-count evidence as an artifact.

For in-memory frontier scaling, dispatch `frontier`. The workflow executes
complete push-and-drain iterations at 100 and 10,000 queued requests and
uploads Criterion's raw `estimates.json` and `sample.json` files. Compare the
`median.point_estimate` or `mean.point_estimate` values from equivalent
artifacts; the values are nanoseconds per complete iteration, so lower is
better.

### Baseline Policy

Committed baselines live in `benchmark/baselines/` and are never overwritten by
the workflow. Update one only after reviewing a successful benchmark artifact
that used the documented toolchain, workload, run count, and concurrency.
Include the baseline update in its own reviewed commit so performance reference
changes remain visible in Git history.

The initial local baseline comes from successful GitHub Actions run
`26966198174`. The initial real-site baseline matches the documented real-site
results above.

## Microbenchmarks

Use Criterion for local microbenchmarks of hot internal paths:

```bash
cargo bench
```

Current microbenchmarks cover:

- request fingerprint canonicalization
- `MemoryFrontier` push/pop overhead at 100 and 10,000 queued requests

For release candidates, run `cargo bench` before tagging and compare the output
against the previous release. CI compiles benchmark targets with
`cargo bench --no-run`, but it does not execute performance measurements because
shared CI runners are too noisy for stable timing.

## Implementations

| Directory | Language | Version |
|-----------|----------|---------|
| `kumo/` | Rust | 1.96.0 / kumo from this repository |
| `scrapy/` | Python | 3.14.5 / uv 0.11.16 / Scrapy 2.16.0 installed with `uv sync` |
| `colly/` | Go | 1.26.3 / Colly v2.3.0 |
| `mockserver/` | nginx | alpine |
