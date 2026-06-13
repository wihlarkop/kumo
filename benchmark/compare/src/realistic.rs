use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const EXPECTED_ITEMS: u64 = 4_000;
const EXPECTED_PAGES: u64 = 200;
const EXPECTED_RETRIES: u64 = 24;
const EXPECTED_REQUESTS: u64 = 224;
const EXPECTED_STATUS_429: u64 = 4;
const EXPECTED_STATUS_503: u64 = 20;

#[derive(Debug, Deserialize)]
struct RawResult {
    elapsed_s: f64,
    items: u64,
    pages: u64,
    errors: u64,
    retries: u64,
    retry_exhausted: u64,
    bytes_downloaded: u64,
    peak_rss_kb: f64,
    peak_in_flight: usize,
}

#[derive(Debug, Deserialize)]
struct Item {
    title: String,
}

#[derive(Debug, Deserialize)]
struct ServerStats {
    workload_requests: u64,
    status_200: u64,
    status_429: u64,
    status_503: u64,
    successful_pages: u64,
    unique_workload_paths: u64,
}

#[derive(Debug, Serialize)]
struct RealisticReport {
    passed: bool,
    failures: Vec<String>,
    items: u64,
    pages: u64,
    duplicates: usize,
    errors: u64,
    retries: u64,
    retry_exhausted: u64,
    retry_recovery_percent: f64,
    elapsed_s: f64,
    items_per_s: f64,
    pages_per_s: f64,
    peak_rss_mb: f64,
    downloaded_mib: f64,
    peak_in_flight: usize,
    server: ServerReport,
}

#[derive(Debug, Serialize)]
struct ServerReport {
    workload_requests: u64,
    status_200: u64,
    status_429: u64,
    status_503: u64,
    unique_workload_paths: u64,
}

struct Args {
    stats: PathBuf,
    items: PathBuf,
    server_stats: PathBuf,
    output: Option<PathBuf>,
    json_output: Option<PathBuf>,
}

pub(crate) fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let raw = load_json::<RawResult>(&args.stats)?;
    let items = load_items(&args.items)?;
    let server = load_json::<ServerStats>(&args.server_stats)?;
    let report = summarize(&raw, &items, &server);
    let markdown = render_markdown(&report);
    print!("{markdown}");

    if let Some(path) = args.output {
        write_output(&path, &markdown)?;
    }
    if let Some(path) = args.json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize realistic report: {error}"))?;
        write_output(&path, &(json + "\n"))?;
    }
    if !report.passed {
        return Err(format!(
            "realistic benchmark validation failed: {}",
            report.failures.join("; ")
        ));
    }
    Ok(())
}

fn summarize(raw: &RawResult, items: &[Item], server: &ServerStats) -> RealisticReport {
    let duplicates = items.len().saturating_sub(
        items
            .iter()
            .map(|item| item.title.as_str())
            .collect::<HashSet<_>>()
            .len(),
    );
    let mut failures = Vec::new();
    check_eq(&mut failures, "crawl items", raw.items, EXPECTED_ITEMS);
    check_eq(
        &mut failures,
        "JSONL rows",
        items.len() as u64,
        EXPECTED_ITEMS,
    );
    check_eq(&mut failures, "crawl pages", raw.pages, EXPECTED_PAGES);
    check_eq(&mut failures, "duplicates", duplicates as u64, 0);
    check_eq(&mut failures, "crawl errors", raw.errors, 0);
    check_eq(&mut failures, "retries", raw.retries, EXPECTED_RETRIES);
    check_eq(&mut failures, "retry exhaustion", raw.retry_exhausted, 0);
    check_eq(
        &mut failures,
        "server workload requests",
        server.workload_requests,
        EXPECTED_REQUESTS,
    );
    check_eq(
        &mut failures,
        "server 200 responses",
        server.status_200,
        EXPECTED_PAGES,
    );
    check_eq(
        &mut failures,
        "server successful pages",
        server.successful_pages,
        EXPECTED_PAGES,
    );
    check_eq(
        &mut failures,
        "server 429 responses",
        server.status_429,
        EXPECTED_STATUS_429,
    );
    check_eq(
        &mut failures,
        "server 503 responses",
        server.status_503,
        EXPECTED_STATUS_503,
    );
    check_eq(
        &mut failures,
        "server unique paths",
        server.unique_workload_paths,
        EXPECTED_PAGES,
    );

    RealisticReport {
        passed: failures.is_empty(),
        failures,
        items: raw.items,
        pages: raw.pages,
        duplicates,
        errors: raw.errors,
        retries: raw.retries,
        retry_exhausted: raw.retry_exhausted,
        retry_recovery_percent: if raw.retries == 0 {
            0.0
        } else {
            (raw.retries.saturating_sub(raw.retry_exhausted)) as f64 / raw.retries as f64 * 100.0
        },
        elapsed_s: raw.elapsed_s,
        items_per_s: raw.items as f64 / raw.elapsed_s,
        pages_per_s: raw.pages as f64 / raw.elapsed_s,
        peak_rss_mb: raw.peak_rss_kb / 1024.0,
        downloaded_mib: raw.bytes_downloaded as f64 / 1024.0 / 1024.0,
        peak_in_flight: raw.peak_in_flight,
        server: ServerReport {
            workload_requests: server.workload_requests,
            status_200: server.status_200,
            status_429: server.status_429,
            status_503: server.status_503,
            unique_workload_paths: server.unique_workload_paths,
        },
    }
}

fn check_eq(failures: &mut Vec<String>, name: &str, actual: u64, expected: u64) {
    if actual != expected {
        failures.push(format!("{name}: expected {expected}, got {actual}"));
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let stats = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let items = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let server_stats = args.next().map(PathBuf::from).ok_or_else(usage)?;
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
        server_stats,
        output,
        json_output,
    })
}

fn usage() -> String {
    "usage: kumo-benchmark-compare realistic <stats.json> <items.jsonl> \
     <server-stats.json> [--output report.md] [--json-output report.json]"
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

fn render_markdown(report: &RealisticReport) -> String {
    let mut output = format!(
        "# Kumo Realistic Resilience Benchmark\n\n\
         | Status | Items | Pages | Duplicates | Errors | Retries | Exhausted | Recovery | Elapsed | Items/s | Pages/s | Peak RSS | Downloaded | Peak in flight |\n\
         |---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n\
         | {} | {} / {} | {} / {} | {} | {} | {} / {} | {} | {:.1}% | {:.3}s | {:.1} | {:.1} | {:.1} MB | {:.1} MiB | {} |\n\n\
         | Server requests | 200 | 429 | 503 | Unique paths |\n\
         |---:|---:|---:|---:|---:|\n\
         | {} | {} | {} | {} | {} |\n",
        if report.passed { "passed" } else { "failed" },
        report.items,
        EXPECTED_ITEMS,
        report.pages,
        EXPECTED_PAGES,
        report.duplicates,
        report.errors,
        report.retries,
        EXPECTED_RETRIES,
        report.retry_exhausted,
        report.retry_recovery_percent,
        report.elapsed_s,
        report.items_per_s,
        report.pages_per_s,
        report.peak_rss_mb,
        report.downloaded_mib,
        report.peak_in_flight,
        report.server.workload_requests,
        report.server.status_200,
        report.server.status_429,
        report.server.status_503,
        report.server.unique_workload_paths,
    );
    if !report.failures.is_empty() {
        output.push_str("\n## Validation Failures\n\n");
        for failure in &report.failures {
            output.push_str(&format!("- {failure}\n"));
        }
    }
    output
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

    fn valid_raw() -> RawResult {
        RawResult {
            elapsed_s: 2.0,
            items: EXPECTED_ITEMS,
            pages: EXPECTED_PAGES,
            errors: 0,
            retries: EXPECTED_RETRIES,
            retry_exhausted: 0,
            bytes_downloaded: 8 * 1024 * 1024,
            peak_rss_kb: 13.0 * 1024.0,
            peak_in_flight: 8,
        }
    }

    fn valid_items() -> Vec<Item> {
        (1..=EXPECTED_ITEMS)
            .map(|number| Item {
                title: format!("Realistic Book {number}"),
            })
            .collect()
    }

    fn valid_server() -> ServerStats {
        ServerStats {
            workload_requests: EXPECTED_REQUESTS,
            status_200: EXPECTED_PAGES,
            status_429: EXPECTED_STATUS_429,
            status_503: EXPECTED_STATUS_503,
            successful_pages: EXPECTED_PAGES,
            unique_workload_paths: EXPECTED_PAGES,
        }
    }

    #[test]
    fn accepts_exact_retry_and_correctness_counts() {
        let report = summarize(&valid_raw(), &valid_items(), &valid_server());

        assert!(report.passed);
        assert!(report.failures.is_empty());
        assert_eq!(report.retry_recovery_percent, 100.0);
        assert!(render_markdown(&report).contains("| passed |"));
    }

    #[test]
    fn rejects_duplicate_items_and_wrong_retry_count() {
        let mut raw = valid_raw();
        raw.retries = EXPECTED_RETRIES - 1;
        let mut items = valid_items();
        items[1].title = items[0].title.clone();

        let report = summarize(&raw, &items, &valid_server());

        assert!(!report.passed);
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("duplicates"))
        );
        assert!(
            report
                .failures
                .iter()
                .any(|failure| failure.contains("retries"))
        );
    }
}
