use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

mod realistic;
mod realistic_compare;
mod scale;
mod soak;
mod store;

const METRICS: [(&str, MetricDirection); 3] = [
    ("elapsed_s", MetricDirection::LowerIsBetter),
    ("items_per_s", MetricDirection::HigherIsBetter),
    ("peak_rss_mb", MetricDirection::LowerIsBetter),
];

#[derive(Debug, Clone, Copy)]
enum MetricDirection {
    LowerIsBetter,
    HigherIsBetter,
}

#[derive(Debug, Deserialize)]
struct BenchmarkResult {
    framework: String,
    #[serde(default)]
    elapsed_s: Option<f64>,
    #[serde(default)]
    items_per_s: Option<f64>,
    #[serde(default)]
    peak_rss_mb: Option<f64>,
    #[serde(default)]
    timings: BTreeMap<String, f64>,
}

impl BenchmarkResult {
    fn metric(&self, name: &str) -> Option<f64> {
        match name {
            "elapsed_s" => self.elapsed_s,
            "items_per_s" => self.items_per_s,
            "peak_rss_mb" => self.peak_rss_mb,
            _ => None,
        }
    }
}

#[derive(Debug, Serialize)]
struct ComparisonReport {
    frameworks: BTreeMap<String, FrameworkComparison>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum FrameworkComparison {
    Compared {
        metrics: BTreeMap<String, MetricDelta>,
        #[serde(skip_serializing_if = "BTreeMap::is_empty")]
        timings: BTreeMap<String, MetricDelta>,
    },
    New,
    MissingCurrent,
}

#[derive(Debug, Serialize)]
struct MetricDelta {
    baseline: Option<f64>,
    current: Option<f64>,
    change: Option<f64>,
    change_percent: Option<f64>,
}

impl MetricDelta {
    fn between(baseline: Option<f64>, current: Option<f64>) -> Self {
        let change = baseline.zip(current).map(|(before, after)| after - before);
        let change_percent = baseline
            .zip(change)
            .filter(|(before, _)| *before != 0.0)
            .map(|(before, delta)| delta / before * 100.0);
        Self {
            baseline,
            current,
            change,
            change_percent,
        }
    }
}

struct Args {
    baseline: PathBuf,
    current: PathBuf,
    output: Option<PathBuf>,
    json_output: Option<PathBuf>,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = args.into_iter();
    let baseline = args.next().map(PathBuf::from).ok_or_else(usage)?;
    let current = args.next().map(PathBuf::from).ok_or_else(usage)?;
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
        baseline,
        current,
        output,
        json_output,
    })
}

fn usage() -> String {
    "usage: kumo-benchmark-compare <baseline.json> <current.json> \
     [--output report.md] [--json-output report.json]"
        .to_string()
}

fn load_results(path: &Path) -> Result<BTreeMap<String, BenchmarkResult>, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let rows: Vec<BenchmarkResult> = serde_json::from_str(contents.trim_start_matches('\u{feff}'))
        .map_err(|error| format!("parse {}: {error}", path.display()))?;
    Ok(rows
        .into_iter()
        .map(|result| (result.framework.clone(), result))
        .collect())
}

fn compare(
    baseline: &BTreeMap<String, BenchmarkResult>,
    current: &BTreeMap<String, BenchmarkResult>,
) -> ComparisonReport {
    let names = baseline
        .keys()
        .chain(current.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let frameworks = names
        .into_iter()
        .map(|name| {
            let comparison = match (baseline.get(&name), current.get(&name)) {
                (None, Some(_)) => FrameworkComparison::New,
                (Some(_), None) => FrameworkComparison::MissingCurrent,
                (Some(before), Some(after)) => {
                    let metrics = METRICS
                        .iter()
                        .map(|(metric, _)| {
                            (
                                (*metric).to_string(),
                                MetricDelta::between(before.metric(metric), after.metric(metric)),
                            )
                        })
                        .collect();
                    let timing_names = before
                        .timings
                        .keys()
                        .chain(after.timings.keys())
                        .filter(|name| name.ends_with("_secs"))
                        .cloned()
                        .collect::<BTreeSet<_>>();
                    let timings = timing_names
                        .into_iter()
                        .map(|timing| {
                            (
                                timing.clone(),
                                MetricDelta::between(
                                    before.timings.get(&timing).copied(),
                                    after.timings.get(&timing).copied(),
                                ),
                            )
                        })
                        .collect();
                    FrameworkComparison::Compared { metrics, timings }
                }
                (None, None) => unreachable!("framework name came from one input"),
            };
            (name, comparison)
        })
        .collect();
    ComparisonReport { frameworks }
}

fn render_markdown(report: &ComparisonReport) -> String {
    let mut lines = vec![
        "# Benchmark Comparison".to_string(),
        String::new(),
        "Shared CI runners are noisy. Treat these deltas as diagnostic signals, not release gates."
            .to_string(),
        String::new(),
        "| Framework | Elapsed | Items/s | Peak RSS |".to_string(),
        "|---|---:|---:|---:|".to_string(),
    ];

    for (framework, comparison) in &report.frameworks {
        match comparison {
            FrameworkComparison::Compared { metrics, .. } => {
                let cells = METRICS
                    .map(|(metric, direction)| format_metric_delta(&metrics[metric], direction));
                lines.push(format!(
                    "| {framework} | {} | {} | {} |",
                    cells[0], cells[1], cells[2]
                ));
            }
            FrameworkComparison::New => {
                lines.push(format!("| {framework} | new | new | new |"));
            }
            FrameworkComparison::MissingCurrent => {
                lines.push(format!(
                    "| {framework} | missing current | missing current | missing current |"
                ));
            }
        }
    }

    if let Some(FrameworkComparison::Compared { timings, .. }) = report.frameworks.get("kumo")
        && !timings.is_empty()
    {
        lines.extend([
            String::new(),
            "## Kumo Phase Timings".to_string(),
            String::new(),
            "| Phase | Baseline (s) | Current (s) | Change |".to_string(),
            "|---|---:|---:|---:|".to_string(),
        ]);
        for (phase, delta) in timings {
            lines.push(format!(
                "| {phase} | {} | {} | {} |",
                format_number(delta.baseline),
                format_number(delta.current),
                format_percent(delta.change_percent)
            ));
        }
    }

    lines.push(String::new());
    lines.join("\n")
}

fn format_metric_delta(delta: &MetricDelta, direction: MetricDirection) -> String {
    let Some(percent) = delta.change_percent else {
        return "n/a".to_string();
    };
    let improved = match direction {
        MetricDirection::LowerIsBetter => percent < 0.0,
        MetricDirection::HigherIsBetter => percent > 0.0,
    };
    let marker = if improved {
        "improved"
    } else if percent == 0.0 {
        "unchanged"
    } else {
        "regressed"
    };
    format!("{} ({marker})", format_percent(Some(percent)))
}

fn format_number(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |value| format!("{value:.3}"))
}

fn format_percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_string(), |value| format!("{value:+.1}%"))
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

fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let report = compare(
        &load_results(&args.baseline)?,
        &load_results(&args.current)?,
    );
    let markdown = render_markdown(&report);
    print!("{markdown}");

    if let Some(path) = args.output {
        write_output(&path, &markdown)?;
    }
    if let Some(path) = args.json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize comparison: {error}"))?;
        write_output(&path, &(json + "\n"))?;
    }
    Ok(())
}

fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    let result = if args.first().is_some_and(|arg| arg == "realistic") {
        args.remove(0);
        realistic::run(args)
    } else if args.first().is_some_and(|arg| arg == "realistic-schedule") {
        args.remove(0);
        realistic_compare::run_schedule(args)
    } else if args.first().is_some_and(|arg| arg == "realistic-compare") {
        args.remove(0);
        realistic_compare::run(args)
    } else if args.first().is_some_and(|arg| arg == "scale") {
        args.remove(0);
        scale::run(args)
    } else if args.first().is_some_and(|arg| arg == "soak") {
        args.remove(0);
        soak::run(args)
    } else if args.first().is_some_and(|arg| arg == "store") {
        args.remove(0);
        store::run(args)
    } else {
        run(args)
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(
        framework: &str,
        elapsed_s: f64,
        items_per_s: f64,
        peak_rss_mb: f64,
        timings: &[(&str, f64)],
    ) -> BenchmarkResult {
        BenchmarkResult {
            framework: framework.to_string(),
            elapsed_s: Some(elapsed_s),
            items_per_s: Some(items_per_s),
            peak_rss_mb: Some(peak_rss_mb),
            timings: timings
                .iter()
                .map(|(name, value)| ((*name).to_string(), *value))
                .collect(),
        }
    }

    #[test]
    fn compares_framework_metrics_and_kumo_timings() {
        let baseline = BTreeMap::from([(
            "kumo".to_string(),
            result(
                "kumo",
                1.0,
                1000.0,
                10.0,
                &[("fetch_secs", 0.8), ("parse_secs", 0.1)],
            ),
        )]);
        let current = BTreeMap::from([(
            "kumo".to_string(),
            result(
                "kumo",
                0.8,
                1250.0,
                11.0,
                &[("fetch_secs", 0.6), ("parse_secs", 0.12)],
            ),
        )]);

        let report = compare(&baseline, &current);
        let FrameworkComparison::Compared { metrics, timings } = &report.frameworks["kumo"] else {
            panic!("kumo should be compared");
        };
        assert!((metrics["elapsed_s"].change_percent.unwrap() + 20.0).abs() < 1e-9);
        assert_eq!(metrics["items_per_s"].change_percent, Some(25.0));
        assert_eq!(metrics["peak_rss_mb"].change_percent, Some(10.0));
        assert!((timings["fetch_secs"].change_percent.unwrap() + 25.0).abs() < 1e-9);
        assert!(render_markdown(&report).contains("Kumo Phase Timings"));
    }

    #[test]
    fn reports_new_and_missing_frameworks_without_failing() {
        let baseline =
            BTreeMap::from([("kumo".to_string(), result("kumo", 1.0, 1000.0, 10.0, &[]))]);
        let current =
            BTreeMap::from([("colly".to_string(), result("colly", 0.5, 2000.0, 20.0, &[]))]);

        let report = compare(&baseline, &current);

        assert!(matches!(
            report.frameworks["kumo"],
            FrameworkComparison::MissingCurrent
        ));
        assert!(matches!(
            report.frameworks["colly"],
            FrameworkComparison::New
        ));
    }
}
