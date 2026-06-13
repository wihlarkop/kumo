use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const SAMPLE_COUNT: usize = 3;

#[derive(Debug, Deserialize)]
struct StoreSample {
    framework: String,
    store_mode: String,
    elapsed_s: f64,
    items: u64,
    pages: u64,
    errors: u64,
    retry_exhausted: u64,
    peak_rss_kb: u64,
    #[serde(default)]
    timings: StoreTimings,
}

#[derive(Debug, Default, Deserialize)]
struct StoreTimings {
    #[serde(default)]
    store_secs: f64,
}

#[derive(Debug, Serialize)]
struct StoreReport {
    expected_items: u64,
    expected_pages: u64,
    samples_per_variant: usize,
    variants: Vec<VariantReport>,
    noop_throughput_gain_percent: f64,
    decision_threshold_percent: f64,
    jsonl_is_material_bottleneck: bool,
}

#[derive(Debug, Serialize)]
struct VariantReport {
    mode: String,
    throughput_items_per_s: RangeMetric,
    elapsed_s: RangeMetric,
    peak_rss_mb: RangeMetric,
    store_s: RangeMetric,
}

#[derive(Debug, Serialize)]
struct RangeMetric {
    median: f64,
    min: f64,
    max: f64,
}

struct Args {
    result_dir: PathBuf,
    expected_items: u64,
    expected_pages: u64,
    output: Option<PathBuf>,
    json_output: Option<PathBuf>,
}

pub(crate) fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let jsonl = load_variant(
        &args.result_dir,
        "jsonl",
        args.expected_items,
        args.expected_pages,
    )?;
    let noop = load_variant(
        &args.result_dir,
        "noop",
        args.expected_items,
        args.expected_pages,
    )?;
    let gain =
        (noop.throughput_items_per_s.median / jsonl.throughput_items_per_s.median - 1.0) * 100.0;
    let report = StoreReport {
        expected_items: args.expected_items,
        expected_pages: args.expected_pages,
        samples_per_variant: SAMPLE_COUNT,
        variants: vec![jsonl, noop],
        noop_throughput_gain_percent: gain,
        decision_threshold_percent: 10.0,
        jsonl_is_material_bottleneck: gain >= 10.0,
    };
    let markdown = render_markdown(&report);
    print!("{markdown}");

    if let Some(path) = args.output {
        super::write_output(&path, &markdown)?;
    }
    if let Some(path) = args.json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize store comparison: {error}"))?;
        super::write_output(&path, &(json + "\n"))?;
    }
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let result_dir = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let expected_items = parse_count(args.next(), "expected items")?;
    let expected_pages = parse_count(args.next(), "expected pages")?;
    let mut output = None;
    let mut json_output = None;

    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a path"))?;
        match flag.as_str() {
            "--output" => output = Some(PathBuf::from(value)),
            "--json-output" => json_output = Some(PathBuf::from(value)),
            _ => return Err(format!("unknown argument: {flag}\n{}", usage())),
        }
    }

    Ok(Args {
        result_dir,
        expected_items,
        expected_pages,
        output,
        json_output,
    })
}

fn parse_count(value: Option<String>, name: &str) -> Result<u64, String> {
    value
        .ok_or_else(usage)?
        .parse::<u64>()
        .map_err(|error| format!("invalid {name}: {error}"))
        .and_then(|value| {
            if value == 0 {
                Err(format!("{name} must be positive"))
            } else {
                Ok(value)
            }
        })
}

fn usage() -> String {
    "usage: kumo-benchmark-compare store <result-dir> <expected-items> \
     <expected-pages> [--output summary.md] [--json-output summary.json]"
        .to_string()
}

fn load_variant(
    result_dir: &Path,
    mode: &str,
    expected_items: u64,
    expected_pages: u64,
) -> Result<VariantReport, String> {
    let mut elapsed = Vec::with_capacity(SAMPLE_COUNT);
    let mut throughput = Vec::with_capacity(SAMPLE_COUNT);
    let mut rss = Vec::with_capacity(SAMPLE_COUNT);
    let mut store = Vec::with_capacity(SAMPLE_COUNT);

    for run in 1..=SAMPLE_COUNT {
        let path = result_dir.join(format!("{mode}_run{run}_stats.json"));
        let contents = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let sample: StoreSample = serde_json::from_str(contents.trim_start_matches('\u{feff}'))
            .map_err(|error| format!("parse {}: {error}", path.display()))?;
        validate_sample(&sample, mode, expected_items, expected_pages, &path)?;

        if mode == "jsonl" {
            let rows_path = result_dir.join(format!("{mode}_run{run}_rows.txt"));
            let rows = fs::read_to_string(&rows_path)
                .map_err(|error| format!("read {}: {error}", rows_path.display()))?
                .trim()
                .parse::<u64>()
                .map_err(|error| format!("parse {}: {error}", rows_path.display()))?;
            if rows != expected_items {
                return Err(format!(
                    "{} contains {rows} rows; expected {expected_items}",
                    rows_path.display()
                ));
            }
        }

        elapsed.push(sample.elapsed_s);
        throughput.push(sample.items as f64 / sample.elapsed_s);
        rss.push(sample.peak_rss_kb as f64 / 1024.0);
        store.push(sample.timings.store_secs);
    }

    Ok(VariantReport {
        mode: mode.to_string(),
        throughput_items_per_s: range_metric(throughput, mode, "throughput")?,
        elapsed_s: range_metric(elapsed, mode, "elapsed time")?,
        peak_rss_mb: range_metric(rss, mode, "peak RSS")?,
        store_s: range_metric(store, mode, "store time")?,
    })
}

fn validate_sample(
    sample: &StoreSample,
    mode: &str,
    expected_items: u64,
    expected_pages: u64,
    path: &Path,
) -> Result<(), String> {
    if sample.framework != "kumo" {
        return Err(format!(
            "{} framework is '{}'; expected 'kumo'",
            path.display(),
            sample.framework
        ));
    }
    if sample.store_mode != mode {
        return Err(format!(
            "{} store mode is '{}'; expected '{mode}'",
            path.display(),
            sample.store_mode
        ));
    }
    if sample.items != expected_items || sample.pages != expected_pages {
        return Err(format!(
            "{} reported {} items and {} pages; expected {expected_items} items and \
             {expected_pages} pages",
            path.display(),
            sample.items,
            sample.pages
        ));
    }
    if sample.errors != 0 || sample.retry_exhausted != 0 {
        return Err(format!(
            "{} reported {} errors and {} exhausted retries",
            path.display(),
            sample.errors,
            sample.retry_exhausted
        ));
    }
    if !sample.elapsed_s.is_finite() || sample.elapsed_s <= 0.0 {
        return Err(format!(
            "{} reported invalid elapsed time {}",
            path.display(),
            sample.elapsed_s
        ));
    }
    if !sample.timings.store_secs.is_finite() || sample.timings.store_secs < 0.0 {
        return Err(format!(
            "{} reported invalid store time {}",
            path.display(),
            sample.timings.store_secs
        ));
    }
    Ok(())
}

fn range_metric(mut values: Vec<f64>, mode: &str, name: &str) -> Result<RangeMetric, String> {
    if values.len() != SAMPLE_COUNT || values.iter().any(|value| !value.is_finite()) {
        return Err(format!("{mode} has invalid {name} samples"));
    }
    values.sort_by(f64::total_cmp);
    Ok(RangeMetric {
        min: values[0],
        median: values[SAMPLE_COUNT / 2],
        max: values[SAMPLE_COUNT - 1],
    })
}

fn render_markdown(report: &StoreReport) -> String {
    let mut lines = vec![
        "# Kumo Store Overhead Benchmark".to_string(),
        String::new(),
        format!(
            "Correctness gate: {} items, {} pages, zero errors, zero exhausted retries.",
            report.expected_items, report.expected_pages
        ),
        String::new(),
        "| Store | Runs | Items/s median (range) | Elapsed median (range) | Peak RSS median (range) | Store time median (range) |".to_string(),
        "|---|---:|---:|---:|---:|---:|".to_string(),
    ];

    for variant in &report.variants {
        lines.push(format!(
            "| {} | {} | {} | {} | {} | {} |",
            variant.mode,
            report.samples_per_variant,
            format_range(&variant.throughput_items_per_s, "", 1),
            format_range(&variant.elapsed_s, " s", 3),
            format_range(&variant.peak_rss_mb, " MB", 1),
            format_range(&variant.store_s, " s", 3),
        ));
    }

    let decision = if report.jsonl_is_material_bottleneck {
        "The JSONL path crosses the 10% threshold; design a bounded batched writer next."
    } else {
        "The JSONL path stays below the 10% threshold; measure URL/domain parsing next."
    };
    lines.extend([
        String::new(),
        format!(
            "No-op throughput difference versus JSONL: **{:+.1}%**.",
            report.noop_throughput_gain_percent
        ),
        String::new(),
        decision.to_string(),
        String::new(),
        "Shared CI runners are noisy; use the balanced repeated ranges as diagnostic evidence."
            .to_string(),
        String::new(),
    ]);
    lines.join("\n")
}

fn format_range(metric: &RangeMetric, suffix: &str, precision: usize) -> String {
    format!(
        "{median:.precision$}{suffix} ({min:.precision$}-{max:.precision$})",
        median = metric.median,
        min = metric.min,
        max = metric.max,
    )
}
