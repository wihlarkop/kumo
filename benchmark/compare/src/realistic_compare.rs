use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const FRAMEWORKS: [&str; 3] = ["kumo", "scrapy", "colly"];
const EXPECTED_ITEMS: u64 = 4_000;
const EXPECTED_PAGES: u64 = 200;
const EXPECTED_RETRIES: u64 = 24;
const EXPECTED_REQUESTS: u64 = 224;
const EXPECTED_STATUS_429: u64 = 4;
const EXPECTED_STATUS_503: u64 = 20;

#[derive(Debug, Deserialize)]
struct FrameworkStats {
    framework: String,
    elapsed_s: f64,
    items: u64,
    pages: u64,
    errors: u64,
    retries: u64,
    retry_exhausted: u64,
    bytes_downloaded: u64,
    peak_rss_kb: f64,
    versions: Versions,
}

#[derive(Debug, Deserialize)]
struct Versions {
    language: String,
    framework: String,
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
struct ComparisonReport {
    passed: bool,
    failures: Vec<String>,
    results: Vec<FrameworkResult>,
}

#[derive(Debug, Serialize)]
struct FrameworkResult {
    framework: String,
    language_version: String,
    framework_version: String,
    items: u64,
    pages: u64,
    duplicates: usize,
    errors: u64,
    retries: u64,
    retry_exhausted: u64,
    elapsed_s: f64,
    items_per_s: f64,
    pages_per_s: f64,
    peak_rss_mb: f64,
    downloaded_mib: f64,
}

struct Args {
    input: PathBuf,
    output: Option<PathBuf>,
    json_output: Option<PathBuf>,
}

pub(crate) fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let mut failures = Vec::new();
    let mut results = Vec::new();
    for framework in FRAMEWORKS {
        let stats =
            load_json::<FrameworkStats>(&args.input.join(format!("{framework}_stats.json")))?;
        let items = load_items(&args.input.join(format!("{framework}.jsonl")))?;
        let server =
            load_json::<ServerStats>(&args.input.join(format!("{framework}_server_stats.json")))?;
        results.push(summarize(framework, &stats, &items, &server, &mut failures));
    }
    let report = ComparisonReport {
        passed: failures.is_empty(),
        failures,
        results,
    };
    let markdown = render_markdown(&report);
    print!("{markdown}");
    if let Some(path) = args.output {
        write_output(&path, &markdown)?;
    }
    if let Some(path) = args.json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize realistic comparison: {error}"))?;
        write_output(&path, &(json + "\n"))?;
    }
    if !report.passed {
        return Err(format!(
            "realistic framework comparison failed: {}",
            report.failures.join("; ")
        ));
    }
    Ok(())
}

fn summarize(
    expected_framework: &str,
    stats: &FrameworkStats,
    items: &[Item],
    server: &ServerStats,
    failures: &mut Vec<String>,
) -> FrameworkResult {
    let unique_titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<HashSet<_>>();
    let duplicates = items.len().saturating_sub(unique_titles.len());
    check_string(
        failures,
        expected_framework,
        "framework label",
        &stats.framework,
        expected_framework,
    );
    check_eq(
        failures,
        expected_framework,
        "crawl items",
        stats.items,
        EXPECTED_ITEMS,
    );
    check_eq(
        failures,
        expected_framework,
        "JSONL rows",
        items.len() as u64,
        EXPECTED_ITEMS,
    );
    check_eq(
        failures,
        expected_framework,
        "crawl pages",
        stats.pages,
        EXPECTED_PAGES,
    );
    check_eq(
        failures,
        expected_framework,
        "duplicates",
        duplicates as u64,
        0,
    );
    check_eq(
        failures,
        expected_framework,
        "final errors",
        stats.errors,
        0,
    );
    check_eq(
        failures,
        expected_framework,
        "retries",
        stats.retries,
        EXPECTED_RETRIES,
    );
    check_eq(
        failures,
        expected_framework,
        "retry exhaustion",
        stats.retry_exhausted,
        0,
    );
    check_eq(
        failures,
        expected_framework,
        "server requests",
        server.workload_requests,
        EXPECTED_REQUESTS,
    );
    check_eq(
        failures,
        expected_framework,
        "server 200 responses",
        server.status_200,
        EXPECTED_PAGES,
    );
    check_eq(
        failures,
        expected_framework,
        "server successful pages",
        server.successful_pages,
        EXPECTED_PAGES,
    );
    check_eq(
        failures,
        expected_framework,
        "server 429 responses",
        server.status_429,
        EXPECTED_STATUS_429,
    );
    check_eq(
        failures,
        expected_framework,
        "server 503 responses",
        server.status_503,
        EXPECTED_STATUS_503,
    );
    check_eq(
        failures,
        expected_framework,
        "server unique paths",
        server.unique_workload_paths,
        EXPECTED_PAGES,
    );

    FrameworkResult {
        framework: stats.framework.clone(),
        language_version: stats.versions.language.clone(),
        framework_version: stats.versions.framework.clone(),
        items: stats.items,
        pages: stats.pages,
        duplicates,
        errors: stats.errors,
        retries: stats.retries,
        retry_exhausted: stats.retry_exhausted,
        elapsed_s: stats.elapsed_s,
        items_per_s: stats.items as f64 / stats.elapsed_s,
        pages_per_s: stats.pages as f64 / stats.elapsed_s,
        peak_rss_mb: stats.peak_rss_kb / 1024.0,
        downloaded_mib: stats.bytes_downloaded as f64 / 1024.0 / 1024.0,
    }
}

fn check_eq(failures: &mut Vec<String>, framework: &str, name: &str, actual: u64, expected: u64) {
    if actual != expected {
        failures.push(format!(
            "{framework} {name}: expected {expected}, got {actual}"
        ));
    }
}

fn check_string(
    failures: &mut Vec<String>,
    framework: &str,
    name: &str,
    actual: &str,
    expected: &str,
) {
    if actual != expected {
        failures.push(format!(
            "{framework} {name}: expected {expected}, got {actual}"
        ));
    }
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let input = args.next().map(PathBuf::from).ok_or_else(usage)?;
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
        input,
        output,
        json_output,
    })
}

fn usage() -> String {
    "usage: kumo-benchmark-compare realistic-compare <results-dir> \
     [--output report.md] [--json-output report.json]"
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

fn render_markdown(report: &ComparisonReport) -> String {
    let mut output = String::from(
        "# Realistic Framework Comparison\n\n\
         Every row passed identical item, page, duplicate, retry, error, and server-counter gates.\n\n\
         | Framework | Runtime | Version | Items/s | Pages/s | Elapsed | Peak RSS | Downloaded | Retries |\n\
         |---|---|---|---:|---:|---:|---:|---:|---:|\n",
    );
    for result in &report.results {
        output.push_str(&format!(
            "| {} | {} | {} | {:.1} | {:.1} | {:.3}s | {:.1} MB | {:.1} MiB | {} |\n",
            result.framework,
            result.language_version,
            result.framework_version,
            result.items_per_s,
            result.pages_per_s,
            result.elapsed_s,
            result.peak_rss_mb,
            result.downloaded_mib,
            result.retries,
        ));
    }
    output.push_str(&format!(
        "\nValidation: **{}**\n",
        if report.passed { "passed" } else { "failed" }
    ));
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

    fn stats(framework: &str) -> FrameworkStats {
        FrameworkStats {
            framework: framework.to_string(),
            elapsed_s: 2.0,
            items: EXPECTED_ITEMS,
            pages: EXPECTED_PAGES,
            errors: 0,
            retries: EXPECTED_RETRIES,
            retry_exhausted: 0,
            bytes_downloaded: 9 * 1024 * 1024,
            peak_rss_kb: 16.0 * 1024.0,
            versions: Versions {
                language: "runtime 1".to_string(),
                framework: format!("{framework} 1"),
            },
        }
    }

    fn items() -> Vec<Item> {
        (1..=EXPECTED_ITEMS)
            .map(|number| Item {
                title: format!("Realistic Book {number}"),
            })
            .collect()
    }

    fn server() -> ServerStats {
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
    fn accepts_framework_with_exact_output_and_retry_counts() {
        let mut failures = Vec::new();
        let result = summarize("kumo", &stats("kumo"), &items(), &server(), &mut failures);

        assert!(failures.is_empty());
        assert_eq!(result.items_per_s, 2_000.0);
    }

    #[test]
    fn rejects_framework_with_incorrect_server_or_output_counts() {
        let mut invalid_stats = stats("scrapy");
        invalid_stats.items -= 1;
        let mut invalid_server = server();
        invalid_server.status_503 -= 1;
        let mut failures = Vec::new();

        summarize(
            "scrapy",
            &invalid_stats,
            &items(),
            &invalid_server,
            &mut failures,
        );

        assert_eq!(failures.len(), 2);
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("crawl items"))
        );
        assert!(
            failures
                .iter()
                .any(|failure| failure.contains("503 responses"))
        );
    }
}
