#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

RUNS=3
LOCAL=false
SCALE=false
WORKLOAD=""
REALISTIC=false
REALISTIC_COMPARE=false
STORE_BENCHMARK=false
CONCURRENCY=16
SEED=0
PAGES_OVERRIDE=0
ITEMS_PER_PAGE_OVERRIDE=20

for arg in "$@"; do
    case $arg in
        --local) LOCAL=true ;;
        --runs=*) RUNS="${arg#*=}" ;;
        --concurrency=*) CONCURRENCY="${arg#*=}" ;;
        --seed=*) SEED="${arg#*=}" ;;
        --pages=*) PAGES_OVERRIDE="${arg#*=}" ;;
        --items-per-page=*) ITEMS_PER_PAGE_OVERRIDE="${arg#*=}" ;;
        --scale) SCALE=true; LOCAL=true ;;
        --soak) WORKLOAD="soak"; LOCAL=true ;;
        --large) WORKLOAD="large"; LOCAL=true ;;
        --realistic) REALISTIC=true ;;
        --realistic-compare) REALISTIC_COMPARE=true ;;
        --store) STORE_BENCHMARK=true; LOCAL=true ;;
    esac
done

if ! [[ "$RUNS" =~ ^[1-9][0-9]*$ ]] \
    || ! [[ "$SEED" =~ ^[0-9]+$ ]] \
    || ! [[ "$PAGES_OVERRIDE" =~ ^[0-9]+$ ]] \
    || ! [[ "$ITEMS_PER_PAGE_OVERRIDE" =~ ^[1-9][0-9]*$ ]]; then
    echo "error: --runs must be positive, --seed and --pages must be zero or greater, and --items-per-page must be positive" >&2
    exit 2
fi

mkdir -p results
export KUMO_VERSION="$(python - <<'EOF'
import re
from pathlib import Path

manifest = Path("..") / "Cargo.toml"
for line in manifest.read_text().splitlines():
    m = re.match(r'version\s*=\s*"([^"]+)"', line)
    if m:
        print(m.group(1))
        break
else:
    print("unknown")
EOF
)"

if [[ "$WORKLOAD" == "soak" ]]; then
    TOTAL_PAGES=500
    ITEMS_PER_PAGE=20
    WORKLOAD_CHAINS=100
elif [[ "$WORKLOAD" == "large" ]]; then
    TOTAL_PAGES=5000
    ITEMS_PER_PAGE=20
    WORKLOAD_CHAINS=100
elif $STORE_BENCHMARK; then
    TOTAL_PAGES=5000
    ITEMS_PER_PAGE=20
    WORKLOAD_CHAINS=100
else
    TOTAL_PAGES=50
    ITEMS_PER_PAGE=20
    WORKLOAD_CHAINS=100
fi
if [[ "$PAGES_OVERRIDE" -gt 0 ]]; then
    TOTAL_PAGES=$PAGES_OVERRIDE
fi
ITEMS_PER_PAGE=$ITEMS_PER_PAGE_OVERRIDE
export TOTAL_PAGES ITEMS_PER_PAGE WORKLOAD_CHAINS

echo "==> Building images..."
echo "    KUMO_VERSION=$KUMO_VERSION"
docker compose build

start_realistic_server() {
    trap 'docker compose stop realisticserver >/dev/null 2>&1 || true' EXIT
    docker compose up -d realisticserver
    for _ in $(seq 1 30); do
        if curl --fail --silent http://localhost:18080/health >/dev/null; then
            break
        fi
        sleep 1
    done
    curl --fail --silent http://localhost:18080/health >/dev/null

    TARGET_URLS=""
    for chain in $(seq 1 20); do
        url="http://realisticserver/realistic/chain-${chain}/page-1.html"
        TARGET_URLS="${TARGET_URLS:+${TARGET_URLS},}${url}"
    done
    export TARGET_URLS
    export REALISTIC_MODE=true
    export CONCURRENCY=$CONCURRENCY
}

stop_realistic_server() {
    docker compose stop realisticserver
    trap - EXIT
}

if $SCALE; then
    echo ""
    echo "==> Kumo scaling benchmark (local mock, concurrency: 1 4 8 16 32 64)..."
    docker compose up -d mockserver
    sleep 1
    TARGET_URLS=""
    for chain in $(seq 1 64); do
        url="http://mockserver/scale/chain-${chain}/page-1.html"
        TARGET_URLS="${TARGET_URLS:+${TARGET_URLS},}${url}"
    done
    export TARGET_URLS
    export SCALE_MODE=true

    mkdir -p results/scale

    for c in 1 4 8 16 32 64; do
        echo ""
        echo "--- concurrency=$c ---"
        export CONCURRENCY=$c
        for i in 1 2 3; do
            echo "    kumo @ concurrency=$c run=$i/3"
            docker compose run --rm kumo
            cp "results/kumo_stats.json" "results/scale/kumo_c${c}_run${i}_stats.json"
        done
    done

    docker compose stop mockserver
    unset CONCURRENCY
    unset TARGET_URLS
    unset SCALE_MODE

    echo ""
    echo "=== Kumo Scaling Results (median of 3 runs) ==="
    cargo run -p kumo-benchmark-compare -- scale results/scale \
        --output results/scale/summary.md \
        --json-output results/scale/summary.json
    exit 0
fi

if $REALISTIC; then
    echo ""
    echo "==> Kumo realistic resilience benchmark..."
    start_realistic_server
    curl --fail --silent --request POST http://localhost:18080/__reset >/dev/null

    rm -f results/kumo.jsonl results/kumo_stats.json results/realistic_server_stats.json
    docker compose run --rm kumo
    curl --fail --silent http://localhost:18080/__stats \
        --output results/realistic_server_stats.json
    mkdir -p results/realistic
    cargo run -p kumo-benchmark-compare -- realistic \
        results/kumo_stats.json \
        results/kumo.jsonl \
        results/realistic_server_stats.json \
        --output results/realistic/summary.md \
        --json-output results/realistic/summary.json
    cp results/kumo_stats.json results/realistic/kumo_stats.json
    cp results/realistic_server_stats.json results/realistic/server_stats.json
    stop_realistic_server
    exit 0
fi

if $REALISTIC_COMPARE; then
    echo ""
    resolved_seed=$SEED
    if [[ "$resolved_seed" == "0" ]]; then
        resolved_seed="${GITHUB_RUN_ID:-$(date +%s)}"
    fi
    echo "==> Realistic framework comparison ($RUNS runs, seed $resolved_seed)..."
    start_realistic_server
    result_dir="results/realistic-compare"
    rm -rf "$result_dir"
    mkdir -p "$result_dir"

    cargo run -p kumo-benchmark-compare -- realistic-schedule \
        "$RUNS" "$resolved_seed" \
        --output "$result_dir/schedule.json" \
        --order-output "$result_dir/schedule.tsv"

    while read -r run svc1 svc2 svc3 <&3; do
        run_dir="$result_dir/run-$run"
        mkdir -p "$run_dir"
        echo ""
        echo "    run $run/$RUNS: $svc1 -> $svc2 -> $svc3"
        for svc in "$svc1" "$svc2" "$svc3"; do
            curl --fail --silent --request POST http://localhost:18080/__reset >/dev/null
            rm -f "results/${svc}.jsonl" "results/${svc}_stats.json"
            docker compose run --rm -T "$svc" </dev/null
            cp "results/${svc}.jsonl" "$run_dir/${svc}.jsonl"
            cp "results/${svc}_stats.json" "$run_dir/${svc}_stats.json"
            curl --fail --silent http://localhost:18080/__stats \
                --output "$run_dir/${svc}_server_stats.json"
        done
    done 3< "$result_dir/schedule.tsv"

    cargo run -p kumo-benchmark-compare -- realistic-compare "$result_dir" \
        --output "$result_dir/summary.md" \
        --json-output "$result_dir/summary.json"
    stop_realistic_server
    exit 0
fi

if $STORE_BENCHMARK; then
    echo ""
    echo "==> Kumo output-store overhead benchmark..."
    docker compose up -d mockserver
    trap 'docker compose stop mockserver >/dev/null 2>&1 || true' EXIT
    sleep 1

    TARGET_URLS=""
    for chain in $(seq 1 "$WORKLOAD_CHAINS"); do
        url="http://mockserver/workload/chain-${chain}/page-1.html"
        TARGET_URLS="${TARGET_URLS:+${TARGET_URLS},}${url}"
    done
    export TARGET_URLS
    export SOAK_MODE=true
    export CONCURRENCY=$CONCURRENCY

    result_dir="results/store"
    rm -rf "$result_dir"
    mkdir -p "$result_dir"
    expected_items=$((TOTAL_PAGES * ITEMS_PER_PAGE))

    for run in 1 2 3; do
        if (( run % 2 == 0 )); then
            variants=(noop jsonl)
        else
            variants=(jsonl noop)
        fi
        echo ""
        echo "    run $run/3: ${variants[0]} -> ${variants[1]}"

        for variant in "${variants[@]}"; do
            export STORE_MODE=$variant
            rm -f results/kumo.jsonl results/kumo_stats.json
            docker compose run --rm kumo
            cp results/kumo_stats.json "$result_dir/${variant}_run${run}_stats.json"

            if [[ "$variant" == "jsonl" ]]; then
                if [[ ! -f results/kumo.jsonl ]]; then
                    echo "error: JSONL benchmark did not create results/kumo.jsonl" >&2
                    exit 1
                fi
                wc -l < results/kumo.jsonl | tr -d ' ' \
                    > "$result_dir/${variant}_run${run}_rows.txt"
            elif [[ -f results/kumo.jsonl ]]; then
                echo "error: no-op benchmark unexpectedly created results/kumo.jsonl" >&2
                exit 1
            fi
        done
    done

    unset STORE_MODE
    cargo run -p kumo-benchmark-compare -- store \
        "$result_dir" \
        "$expected_items" \
        "$TOTAL_PAGES" \
        --output "$result_dir/summary.md" \
        --json-output "$result_dir/summary.json"
    docker compose stop mockserver
    trap - EXIT
    exit 0
fi

if [[ -n "$WORKLOAD" ]]; then
    echo ""
    echo "==> Kumo $WORKLOAD validation ($TOTAL_PAGES pages, $ITEMS_PER_PAGE items/page)..."
    docker compose up -d mockserver
    sleep 1
    active_chains=$WORKLOAD_CHAINS
    if [[ "$TOTAL_PAGES" -lt "$active_chains" ]]; then
        active_chains=$TOTAL_PAGES
    fi
    TARGET_URLS=""
    for chain in $(seq 1 "$active_chains"); do
        url="http://mockserver/workload/chain-${chain}/page-1.html"
        TARGET_URLS="${TARGET_URLS:+${TARGET_URLS},}${url}"
    done
    export TARGET_URLS
    export SOAK_MODE=true
    export CONCURRENCY=$CONCURRENCY
    expected_items=$((TOTAL_PAGES * ITEMS_PER_PAGE))

    rm -f results/kumo.jsonl results/kumo_stats.json
    docker compose run --rm kumo
    mkdir -p "results/$WORKLOAD"
    cargo run -p kumo-benchmark-compare -- soak \
        results/kumo_stats.json \
        results/kumo.jsonl \
        "$expected_items" \
        "$TOTAL_PAGES" \
        --output "results/$WORKLOAD/summary.md" \
        --json-output "results/$WORKLOAD/summary.json"
    cp results/kumo_stats.json "results/$WORKLOAD/kumo_stats.json"
    docker compose stop mockserver
    exit 0
fi

if $LOCAL; then
    echo ""
    echo "==> Starting mock server..."
    docker compose up -d mockserver
    sleep 1
    export TARGET_URL="http://mockserver/catalogue/page-1.html"
    echo "    TARGET_URL=$TARGET_URL"
fi

export CONCURRENCY=$CONCURRENCY
echo "    CONCURRENCY=$CONCURRENCY"

for svc in kumo scrapy colly; do
    echo ""
    echo "==> Running $svc ($RUNS runs)..."
    for i in $(seq 1 "$RUNS"); do
        echo "    run $i/$RUNS"
        docker compose run --rm "$svc"
        cp "results/${svc}_stats.json" "results/${svc}_run${i}_stats.json"
    done
done

if $LOCAL; then
    docker compose stop mockserver
fi

echo ""
echo "=== Benchmark Results (median of $RUNS runs) ==="

python - <<EOF
import json, os, statistics

RUNS = $RUNS
services = ["kumo", "scrapy", "colly"]
rows = []

for name in services:
    elapsed_vals, rss_vals, item_vals, page_vals = [], [], [], []
    timing_vals = {}
    extraction_operation_vals = {}
    versions = {}
    concurrency = None
    for i in range(1, RUNS + 1):
        path = f"results/{name}_run{i}_stats.json"
        if not os.path.exists(path):
            continue
        with open(path) as f:
            s = json.load(f)
        elapsed_vals.append(s.get("elapsed_s", 0))
        rss_vals.append(s.get("peak_rss_kb", 0))
        item_vals.append(int(s.get("items", 0)))
        page_vals.append(int(s.get("pages", 0)))
        for key, value in s.get("timings", {}).items():
            if isinstance(value, (int, float)):
                timing_vals.setdefault(key, []).append(value)
        for key, value in s.get("extraction_operations", {}).items():
            if isinstance(value, (int, float)):
                extraction_operation_vals.setdefault(key, []).append(value)
        versions = versions or s.get("versions", {})
        concurrency = concurrency or s.get("concurrency")

    if not elapsed_vals:
        continue

    elapsed = statistics.median(elapsed_vals)
    rss_kb  = statistics.median(rss_vals)
    items   = statistics.median(item_vals)
    pages   = statistics.median(page_vals) if page_vals else 0
    rps     = round(items / elapsed, 1) if elapsed > 0 else 0
    rss_mb  = round(rss_kb / 1024, 1)
    timings = {
        key: statistics.median(values)
        for key, values in timing_vals.items()
        if values
    }
    extraction_operations = {
        key: int(statistics.median(values))
        for key, values in extraction_operation_vals.items()
        if values
    }
    rows.append((
        name,
        int(items),
        int(pages),
        elapsed,
        rps,
        rss_mb,
        concurrency,
        versions,
        timings,
        extraction_operations,
    ))

print(f"{'Framework':<12} {'Items':>8} {'Pages':>8} {'Time (s)':>10} {'Items/s':>10} {'RSS (MB)':>10}")
print("-" * 64)
for name, items, pages, elapsed, rps, rss_mb, concurrency, versions, timings, extraction_operations in rows:
    print(f"{name:<12} {items:>8} {pages:>8} {elapsed:>10.2f} {rps:>10.1f} {rss_mb:>10.1f}")

print()
output = [
    {
        "framework": n,
        "items": i,
        "pages": p,
        "elapsed_s": e,
        "items_per_s": r,
        "peak_rss_mb": m,
        "concurrency": c,
        "versions": v,
        **({"timings": t} if t else {}),
        **({"extraction_operations": x} if x else {}),
    }
    for n, i, p, e, r, m, c, v, t, x in rows
]
suffix = "_local" if "$LOCAL" == "true" else ""
out_path = f"results/latest{suffix}.json"
with open(out_path, "w") as f:
    json.dump(output, f, indent=2)
print(f"Results saved to {out_path}")
EOF
