use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const FRAMEWORKS: [&str; 3] = ["kumo", "scrapy", "colly"];
const PERMUTATIONS: [[&str; 3]; 6] = [
    ["kumo", "scrapy", "colly"],
    ["kumo", "colly", "scrapy"],
    ["scrapy", "kumo", "colly"],
    ["scrapy", "colly", "kumo"],
    ["colly", "kumo", "scrapy"],
    ["colly", "scrapy", "kumo"],
];
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

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ComparisonSchedule {
    seed: u64,
    runs: Vec<ScheduledRun>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ScheduledRun {
    run: usize,
    order: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ComparisonReport {
    passed: bool,
    failures: Vec<String>,
    schedule: ComparisonSchedule,
    results: Vec<FrameworkAggregate>,
}

#[derive(Debug, Serialize)]
struct FrameworkAggregate {
    framework: String,
    language_version: String,
    framework_version: String,
    run_count: usize,
    median_elapsed_s: f64,
    median_items_per_s: f64,
    min_items_per_s: f64,
    max_items_per_s: f64,
    median_pages_per_s: f64,
    median_peak_rss_mb: f64,
    min_peak_rss_mb: f64,
    max_peak_rss_mb: f64,
    median_downloaded_mib: f64,
    runs: Vec<FrameworkRunResult>,
}

#[derive(Debug, Serialize)]
struct FrameworkRunResult {
    run: usize,
    position: usize,
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

struct ScheduleArgs {
    runs: usize,
    seed: u64,
    output: Option<PathBuf>,
    order_output: Option<PathBuf>,
}

pub(crate) fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let schedule = load_json::<ComparisonSchedule>(&args.input.join("schedule.json"))?;
    validate_schedule(&schedule)?;

    let mut failures = Vec::new();
    let mut samples = FRAMEWORKS
        .iter()
        .map(|framework| (*framework, Vec::new()))
        .collect::<std::collections::HashMap<_, _>>();

    for scheduled_run in &schedule.runs {
        let run_dir = args.input.join(format!("run-{}", scheduled_run.run));
        for (position, framework) in scheduled_run.order.iter().enumerate() {
            let stats =
                load_json::<FrameworkStats>(&run_dir.join(format!("{framework}_stats.json")))?;
            let items = load_items(&run_dir.join(format!("{framework}.jsonl")))?;
            let server =
                load_json::<ServerStats>(&run_dir.join(format!("{framework}_server_stats.json")))?;
            let sample = summarize_run(
                scheduled_run.run,
                position + 1,
                framework,
                &stats,
                &items,
                &server,
                &mut failures,
            );
            samples
                .get_mut(framework.as_str())
                .ok_or_else(|| format!("unknown framework in schedule: {framework}"))?
                .push(sample);
        }
    }

    let results = FRAMEWORKS
        .iter()
        .map(|framework| {
            aggregate_framework(
                framework,
                samples.remove(*framework).unwrap_or_default(),
                &mut failures,
            )
        })
        .collect();
    let report = ComparisonReport {
        passed: failures.is_empty(),
        failures,
        schedule,
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

pub(crate) fn run_schedule(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_schedule_args(args)?;
    let schedule = generate_schedule(args.runs, args.seed)?;
    let json = serde_json::to_string_pretty(&schedule)
        .map_err(|error| format!("serialize realistic schedule: {error}"))?
        + "\n";
    if let Some(path) = args.output {
        write_output(&path, &json)?;
    } else {
        print!("{json}");
    }
    if let Some(path) = args.order_output {
        let mut orders = String::new();
        for run in &schedule.runs {
            orders.push_str(&format!("{} {}\n", run.run, run.order.join(" ")));
        }
        write_output(&path, &orders)?;
    }
    Ok(())
}

fn generate_schedule(runs: usize, seed: u64) -> Result<ComparisonSchedule, String> {
    if runs == 0 {
        return Err("realistic comparison requires at least one run".to_string());
    }
    let base = PERMUTATIONS[(seed % PERMUTATIONS.len() as u64) as usize];
    let runs = (0..runs)
        .map(|index| ScheduledRun {
            run: index + 1,
            order: (0..FRAMEWORKS.len())
                .map(|offset| base[(offset + index) % FRAMEWORKS.len()].to_string())
                .collect(),
        })
        .collect();
    Ok(ComparisonSchedule { seed, runs })
}

fn validate_schedule(schedule: &ComparisonSchedule) -> Result<(), String> {
    if schedule.runs.is_empty() {
        return Err("realistic comparison schedule has no runs".to_string());
    }
    let expected = FRAMEWORKS.into_iter().collect::<HashSet<_>>();
    for (index, run) in schedule.runs.iter().enumerate() {
        if run.run != index + 1 {
            return Err(format!(
                "schedule run number mismatch: expected {}, got {}",
                index + 1,
                run.run
            ));
        }
        let actual = run.order.iter().map(String::as_str).collect::<HashSet<_>>();
        if run.order.len() != FRAMEWORKS.len() || actual != expected {
            return Err(format!(
                "schedule run {} must contain each framework exactly once",
                run.run
            ));
        }
    }
    Ok(())
}

fn summarize_run(
    run: usize,
    position: usize,
    expected_framework: &str,
    stats: &FrameworkStats,
    items: &[Item],
    server: &ServerStats,
    failures: &mut Vec<String>,
) -> FrameworkRunResult {
    let label = format!("run {run} {expected_framework}");
    let unique_titles = items
        .iter()
        .map(|item| item.title.as_str())
        .collect::<HashSet<_>>();
    let duplicates = items.len().saturating_sub(unique_titles.len());
    check_string(
        failures,
        &label,
        "framework label",
        &stats.framework,
        expected_framework,
    );
    check_eq(failures, &label, "crawl items", stats.items, EXPECTED_ITEMS);
    check_eq(
        failures,
        &label,
        "JSONL rows",
        items.len() as u64,
        EXPECTED_ITEMS,
    );
    check_eq(failures, &label, "crawl pages", stats.pages, EXPECTED_PAGES);
    check_eq(failures, &label, "duplicates", duplicates as u64, 0);
    check_eq(failures, &label, "final errors", stats.errors, 0);
    check_eq(failures, &label, "retries", stats.retries, EXPECTED_RETRIES);
    check_eq(
        failures,
        &label,
        "retry exhaustion",
        stats.retry_exhausted,
        0,
    );
    check_eq(
        failures,
        &label,
        "server requests",
        server.workload_requests,
        EXPECTED_REQUESTS,
    );
    check_eq(
        failures,
        &label,
        "server 200 responses",
        server.status_200,
        EXPECTED_PAGES,
    );
    check_eq(
        failures,
        &label,
        "server successful pages",
        server.successful_pages,
        EXPECTED_PAGES,
    );
    check_eq(
        failures,
        &label,
        "server 429 responses",
        server.status_429,
        EXPECTED_STATUS_429,
    );
    check_eq(
        failures,
        &label,
        "server 503 responses",
        server.status_503,
        EXPECTED_STATUS_503,
    );
    check_eq(
        failures,
        &label,
        "server unique paths",
        server.unique_workload_paths,
        EXPECTED_PAGES,
    );
    if !stats.elapsed_s.is_finite() || stats.elapsed_s <= 0.0 {
        failures.push(format!(
            "{label} elapsed time must be finite and positive, got {}",
            stats.elapsed_s
        ));
    }
    let elapsed_s = stats.elapsed_s.max(f64::MIN_POSITIVE);

    FrameworkRunResult {
        run,
        position,
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
        items_per_s: stats.items as f64 / elapsed_s,
        pages_per_s: stats.pages as f64 / elapsed_s,
        peak_rss_mb: stats.peak_rss_kb / 1024.0,
        downloaded_mib: stats.bytes_downloaded as f64 / 1024.0 / 1024.0,
    }
}

fn aggregate_framework(
    framework: &str,
    runs: Vec<FrameworkRunResult>,
    failures: &mut Vec<String>,
) -> FrameworkAggregate {
    if runs.is_empty() {
        failures.push(format!("{framework} has no benchmark runs"));
        return FrameworkAggregate {
            framework: framework.to_string(),
            language_version: String::new(),
            framework_version: String::new(),
            run_count: 0,
            median_elapsed_s: 0.0,
            median_items_per_s: 0.0,
            min_items_per_s: 0.0,
            max_items_per_s: 0.0,
            median_pages_per_s: 0.0,
            median_peak_rss_mb: 0.0,
            min_peak_rss_mb: 0.0,
            max_peak_rss_mb: 0.0,
            median_downloaded_mib: 0.0,
            runs,
        };
    }

    let language_version = runs[0].language_version.clone();
    let framework_version = runs[0].framework_version.clone();
    for run in &runs[1..] {
        if run.language_version != language_version {
            failures.push(format!(
                "{framework} language version drift: expected {language_version}, got {} in run {}",
                run.language_version, run.run
            ));
        }
        if run.framework_version != framework_version {
            failures.push(format!(
                "{framework} framework version drift: expected {framework_version}, got {} in run {}",
                run.framework_version, run.run
            ));
        }
    }

    let elapsed = runs.iter().map(|run| run.elapsed_s).collect::<Vec<_>>();
    let throughput = runs.iter().map(|run| run.items_per_s).collect::<Vec<_>>();
    let page_rate = runs.iter().map(|run| run.pages_per_s).collect::<Vec<_>>();
    let rss = runs.iter().map(|run| run.peak_rss_mb).collect::<Vec<_>>();
    let downloaded = runs
        .iter()
        .map(|run| run.downloaded_mib)
        .collect::<Vec<_>>();

    FrameworkAggregate {
        framework: framework.to_string(),
        language_version,
        framework_version,
        run_count: runs.len(),
        median_elapsed_s: median(&elapsed),
        median_items_per_s: median(&throughput),
        min_items_per_s: minimum(&throughput),
        max_items_per_s: maximum(&throughput),
        median_pages_per_s: median(&page_rate),
        median_peak_rss_mb: median(&rss),
        min_peak_rss_mb: minimum(&rss),
        max_peak_rss_mb: maximum(&rss),
        median_downloaded_mib: median(&downloaded),
        runs,
    }
}

fn median(values: &[f64]) -> f64 {
    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[middle - 1] + values[middle]) / 2.0
    } else {
        values[middle]
    }
}

fn minimum(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::INFINITY, f64::min)
}

fn maximum(values: &[f64]) -> f64 {
    values.iter().copied().fold(f64::NEG_INFINITY, f64::max)
}

fn check_eq(failures: &mut Vec<String>, label: &str, name: &str, actual: u64, expected: u64) {
    if actual != expected {
        failures.push(format!("{label} {name}: expected {expected}, got {actual}"));
    }
}

fn check_string(failures: &mut Vec<String>, label: &str, name: &str, actual: &str, expected: &str) {
    if actual != expected {
        failures.push(format!("{label} {name}: expected {expected}, got {actual}"));
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

fn parse_schedule_args(args: impl IntoIterator<Item = String>) -> Result<ScheduleArgs, String> {
    let mut args = args.into_iter();
    let runs = args
        .next()
        .ok_or_else(schedule_usage)?
        .parse::<usize>()
        .map_err(|error| format!("invalid run count: {error}"))?;
    let seed = args
        .next()
        .ok_or_else(schedule_usage)?
        .parse::<u64>()
        .map_err(|error| format!("invalid schedule seed: {error}"))?;
    let mut output = None;
    let mut order_output = None;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("{flag} requires a path"))?;
        match flag.as_str() {
            "--output" => output = Some(PathBuf::from(value)),
            "--order-output" => order_output = Some(PathBuf::from(value)),
            _ => {
                return Err(format!("unknown argument: {flag}\n{}", schedule_usage()));
            }
        }
    }
    Ok(ScheduleArgs {
        runs,
        seed,
        output,
        order_output,
    })
}

fn usage() -> String {
    "usage: kumo-benchmark-compare realistic-compare <results-dir> \
     [--output report.md] [--json-output report.json]"
        .to_string()
}

fn schedule_usage() -> String {
    "usage: kumo-benchmark-compare realistic-schedule <runs> <seed> \
     [--output schedule.json] [--order-output schedule.tsv]"
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
    let mut output = format!(
        "# Realistic Framework Comparison\n\n\
         Every framework sample across {} runs is checked against identical item, page, duplicate, retry, error, and server-counter gates.\n\n\
         Seed: `{}`\n\n\
         ## Execution Order\n\n",
        report.schedule.runs.len(),
        report.schedule.seed
    );
    for run in &report.schedule.runs {
        output.push_str(&format!("- Run {}: {}\n", run.run, run.order.join(" -> ")));
    }
    output.push_str(
        "\n## Median Results\n\n\
         | Framework | Runtime | Version | Runs | Items/s median (range) | Pages/s | Elapsed | Peak RSS median (range) | Downloaded |\n\
         |---|---|---|---:|---:|---:|---:|---:|---:|\n",
    );
    for result in &report.results {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {:.1} ({:.1}-{:.1}) | {:.1} | {:.3}s | {:.1} MB ({:.1}-{:.1}) | {:.1} MiB |\n",
            result.framework,
            result.language_version,
            result.framework_version,
            result.run_count,
            result.median_items_per_s,
            result.min_items_per_s,
            result.max_items_per_s,
            result.median_pages_per_s,
            result.median_elapsed_s,
            result.median_peak_rss_mb,
            result.min_peak_rss_mb,
            result.max_peak_rss_mb,
            result.median_downloaded_mib,
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
        let result = summarize_run(
            1,
            1,
            "kumo",
            &stats("kumo"),
            &items(),
            &server(),
            &mut failures,
        );

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

        summarize_run(
            1,
            1,
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
