use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
struct RawResult {
    elapsed_s: f64,
    items: u64,
    pages: u64,
    errors: u64,
    peak_rss_kb: f64,
    first_10k_elapsed_s: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct Item {
    title: String,
}

#[derive(Debug, Serialize)]
struct SoakReport {
    expected_items: u64,
    actual_items: u64,
    expected_pages: u64,
    actual_pages: u64,
    duplicate_items: usize,
    crawl_errors: u64,
    elapsed_s: f64,
    items_per_s: f64,
    peak_rss_mb: f64,
    rss_mb_per_1k_items: f64,
    first_10k_items_per_s: Option<f64>,
    after_10k_items_per_s: Option<f64>,
    after_10k_throughput_change_percent: Option<f64>,
    passed: bool,
}

struct Args {
    stats: PathBuf,
    items: PathBuf,
    expected_items: u64,
    expected_pages: u64,
    output: Option<PathBuf>,
    json_output: Option<PathBuf>,
}

pub(crate) fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let raw = load_json::<RawResult>(&args.stats)?;
    let items = load_items(&args.items)?;
    let unique_titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<HashSet<_>>();
    let duplicate_items = items.len().saturating_sub(unique_titles.len());
    let first_rate = raw
        .first_10k_elapsed_s
        .filter(|elapsed| *elapsed > 0.0)
        .map(|elapsed| 10_000.0 / elapsed);
    let after_rate = raw.first_10k_elapsed_s.and_then(|first_elapsed| {
        (raw.items > 10_000 && raw.elapsed_s > first_elapsed)
            .then(|| (raw.items - 10_000) as f64 / (raw.elapsed_s - first_elapsed))
    });
    let after_change = first_rate
        .zip(after_rate)
        .map(|(first, after)| (after - first) / first * 100.0);
    let passed = raw.items == args.expected_items
        && items.len() as u64 == args.expected_items
        && raw.pages == args.expected_pages
        && duplicate_items == 0
        && raw.errors == 0;
    let peak_rss_mb = raw.peak_rss_kb / 1024.0;
    let report = SoakReport {
        expected_items: args.expected_items,
        actual_items: raw.items,
        expected_pages: args.expected_pages,
        actual_pages: raw.pages,
        duplicate_items,
        crawl_errors: raw.errors,
        elapsed_s: raw.elapsed_s,
        items_per_s: raw.items as f64 / raw.elapsed_s,
        peak_rss_mb,
        rss_mb_per_1k_items: peak_rss_mb / (raw.items as f64 / 1_000.0),
        first_10k_items_per_s: first_rate,
        after_10k_items_per_s: after_rate,
        after_10k_throughput_change_percent: after_change,
        passed,
    };
    let markdown = render_markdown(&report);
    print!("{markdown}");
    if let Some(path) = args.output {
        write_output(&path, &markdown)?;
    }
    if let Some(path) = args.json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize soak report: {error}"))?;
        write_output(&path, &(json + "\n"))?;
    }
    if !passed {
        return Err("large-crawl correctness validation failed".to_string());
    }
    Ok(())
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let stats = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let items = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let expected_items = parse_u64(args.next(), "expected items")?;
    let expected_pages = parse_u64(args.next(), "expected pages")?;
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
        stats,
        items,
        expected_items,
        expected_pages,
        output,
        json_output,
    })
}

fn parse_u64(value: Option<String>, name: &str) -> Result<u64, String> {
    value
        .ok_or_else(usage)?
        .parse()
        .map_err(|error| format!("invalid {name}: {error}"))
}

fn usage() -> String {
    "usage: kumo-benchmark-compare soak <stats.json> <items.jsonl> \
     <expected-items> <expected-pages> [--output report.md] [--json-output report.json]"
        .to_string()
}

fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&contents).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn load_items(path: &Path) -> Result<Vec<Item>, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    contents
        .lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty())
        .map(|(index, line)| {
            serde_json::from_str(line)
                .map_err(|error| format!("parse {} line {}: {error}", path.display(), index + 1))
        })
        .collect()
}

fn render_markdown(report: &SoakReport) -> String {
    format!(
        "# Kumo Large-Crawl Validation\n\n\
         | Status | Items | Pages | Duplicates | Errors | Items/s | Peak RSS | RSS / 1k items |\n\
         |---|---:|---:|---:|---:|---:|---:|---:|\n\
         | {} | {} / {} | {} / {} | {} | {} | {:.1} | {:.1} MB | {:.3} MB |\n\n\
         First 10k throughput: {}\n\n\
         After 10k throughput: {}\n\n\
         After 10k change: {}\n",
        if report.passed { "passed" } else { "failed" },
        report.actual_items,
        report.expected_items,
        report.actual_pages,
        report.expected_pages,
        report.duplicate_items,
        report.crawl_errors,
        report.items_per_s,
        report.peak_rss_mb,
        report.rss_mb_per_1k_items,
        format_rate(report.first_10k_items_per_s),
        format_rate(report.after_10k_items_per_s),
        report
            .after_10k_throughput_change_percent
            .map_or_else(|| "n/a".to_string(), |value| format!("{value:+.1}%")),
    )
}

fn format_rate(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |value| format!("{value:.1} items/s"))
}

fn write_output(path: &Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_memory_and_throughput_metrics() {
        let report = SoakReport {
            expected_items: 100_000,
            actual_items: 100_000,
            expected_pages: 5_000,
            actual_pages: 5_000,
            duplicate_items: 0,
            crawl_errors: 0,
            elapsed_s: 2.0,
            items_per_s: 50_000.0,
            peak_rss_mb: 20.0,
            rss_mb_per_1k_items: 0.2,
            first_10k_items_per_s: Some(40_000.0),
            after_10k_items_per_s: Some(51_000.0),
            after_10k_throughput_change_percent: Some(27.5),
            passed: true,
        };

        let markdown = render_markdown(&report);
        assert!(markdown.contains("passed"));
        assert!(markdown.contains("0.200 MB"));
        assert!(markdown.contains("+27.5%"));
    }
}
