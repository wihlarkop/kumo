#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

RUNS=3
LOCAL=false
SCALE=false
CONCURRENCY=16

for arg in "$@"; do
    case $arg in
        --local) LOCAL=true ;;
        --runs=*) RUNS="${arg#*=}" ;;
        --concurrency=*) CONCURRENCY="${arg#*=}" ;;
        --scale) SCALE=true; LOCAL=true ;;
    esac
done

mkdir -p results

echo "==> Building images..."
docker compose build

if $SCALE; then
    echo ""
    echo "==> Scaling benchmark (local mock, concurrency: 16 32 64 128)..."
    docker compose up -d mockserver
    sleep 1
    export TARGET_URL="http://mockserver/catalogue/page-1.html"

    mkdir -p results/scale

    for c in 16 32 64 128; do
        echo ""
        echo "--- concurrency=$c ---"
        export CONCURRENCY=$c
        for svc in kumo scrapy colly; do
            echo "    $svc @ concurrency=$c"
            docker compose run --rm "$svc"
            cp "results/${svc}_stats.json" "results/scale/${svc}_c${c}_stats.json"
        done
    done

    docker compose stop mockserver
    unset CONCURRENCY

    echo ""
    echo "=== Scaling Results (items/s) ==="
    python - <<EOF
import json, os

services = ["kumo", "scrapy", "colly"]
levels = [16, 32, 64, 128]

print(f"{'Concurrency':>13}", end="")
for svc in services:
    print(f"  {svc:>12}", end="")
print()
print("-" * (13 + 14 * len(services)))

for c in levels:
    print(f"{c:>13}", end="")
    for svc in services:
        path = f"results/scale/{svc}_c{c}_stats.json"
        if os.path.exists(path):
            with open(path) as f:
                s = json.load(f)
            rps = round(s.get("items", 0) / s.get("elapsed_s", 1), 0)
            print(f"  {rps:>12.0f}", end="")
        else:
            print(f"  {'n/a':>12}", end="")
    print()

print()
print("(items/s per framework at each concurrency level, local mock server)")
EOF
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
    elapsed_vals, rss_vals, item_vals = [], [], []
    for i in range(1, RUNS + 1):
        path = f"results/{name}_run{i}_stats.json"
        if not os.path.exists(path):
            continue
        with open(path) as f:
            s = json.load(f)
        elapsed_vals.append(s.get("elapsed_s", 0))
        rss_vals.append(s.get("peak_rss_kb", 0))
        item_vals.append(int(s.get("items", 0)))

    if not elapsed_vals:
        continue

    elapsed = statistics.median(elapsed_vals)
    rss_kb  = statistics.median(rss_vals)
    items   = statistics.median(item_vals)
    rps     = round(items / elapsed, 1) if elapsed > 0 else 0
    rss_mb  = round(rss_kb / 1024, 1)
    rows.append((name, int(items), elapsed, rps, rss_mb))

print(f"{'Framework':<12} {'Items':>8} {'Time (s)':>10} {'Items/s':>10} {'RSS (MB)':>10}")
print("-" * 54)
for name, items, elapsed, rps, rss_mb in rows:
    print(f"{name:<12} {items:>8} {elapsed:>10.2f} {rps:>10.1f} {rss_mb:>10.1f}")

print()
output = [
    {"framework": n, "items": i, "elapsed_s": e, "items_per_s": r, "peak_rss_mb": m}
    for n, i, e, r, m in rows
]
suffix = "_local" if "$LOCAL" == "true" else ""
out_path = f"results/latest{suffix}.json"
with open(out_path, "w") as f:
    json.dump(output, f, indent=2)
print(f"Results saved to {out_path}")
EOF
