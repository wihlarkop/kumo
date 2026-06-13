use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const LEVELS: [usize; 6] = [1, 4, 8, 16, 32, 64];
const RUNS_PER_LEVEL: usize = 3;

#[derive(Debug, Deserialize)]
struct RawResult {
    elapsed_s: f64,
    items: u64,
    pages: u64,
    peak_rss_kb: f64,
    peak_in_flight: usize,
    #[serde(default)]
    timings: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct ScaleReport {
    runs_per_level: usize,
    results: Vec<ScaleResult>,
    saturation: Option<Saturation>,
}

#[derive(Debug, Serialize)]
struct ScaleResult {
    concurrency: usize,
    items: u64,
    pages: u64,
    elapsed_s: f64,
    items_per_s: f64,
    peak_rss_mb: f64,
    peak_in_flight: usize,
    fetch_secs: f64,
    parse_secs: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    throughput_change_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    rss_change_percent: Option<f64>,
}

#[derive(Debug, Serialize)]
struct Saturation {
    concurrency: usize,
    reason: &'static str,
}

struct Args {
    input: PathBuf,
    output: Option<PathBuf>,
    json_output: Option<PathBuf>,
}

pub(crate) fn run(args: impl IntoIterator<Item = String>) -> Result<(), String> {
    let args = parse_args(args)?;
    let mut grouped = Vec::new();
    for concurrency in LEVELS {
        let runs = (1..=RUNS_PER_LEVEL)
            .map(|run| {
                let path = args
                    .input
                    .join(format!("kumo_c{concurrency}_run{run}_stats.json"));
                load_result(&path)
            })
            .collect::<Result<Vec<_>, _>>()?;
        grouped.push((concurrency, runs));
    }

    let report = summarize(&grouped);
    let markdown = render_markdown(&report);
    print!("{markdown}");
    if let Some(path) = args.output {
        write_output(&path, &markdown)?;
    }
    if let Some(path) = args.json_output {
        let json = serde_json::to_string_pretty(&report)
            .map_err(|error| format!("serialize scale report: {error}"))?;
        write_output(&path, &(json + "\n"))?;
    }
    Ok(())
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
    "usage: kumo-benchmark-compare scale <results-dir> \
     [--output report.md] [--json-output report.json]"
        .to_string()
}

fn load_result(path: &Path) -> Result<RawResult, String> {
    let contents =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    serde_json::from_str(&contents).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn summarize(grouped: &[(usize, Vec<RawResult>)]) -> ScaleReport {
    let mut results = grouped
        .iter()
        .map(|(concurrency, runs)| {
            let elapsed_s = median(runs.iter().map(|run| run.elapsed_s));
            let items = median(runs.iter().map(|run| run.items as f64)) as u64;
            let pages = median(runs.iter().map(|run| run.pages as f64)) as u64;
            let peak_rss_mb = median(runs.iter().map(|run| run.peak_rss_kb)) / 1024.0;
            let peak_in_flight = median(runs.iter().map(|run| run.peak_in_flight as f64)) as usize;
            ScaleResult {
                concurrency: *concurrency,
                items,
                pages,
                elapsed_s,
                items_per_s: items as f64 / elapsed_s,
                peak_rss_mb,
                peak_in_flight,
                fetch_secs: median(
                    runs.iter()
                        .map(|run| run.timings.get("fetch_secs").copied().unwrap_or(0.0)),
                ),
                parse_secs: median(
                    runs.iter()
                        .map(|run| run.timings.get("parse_secs").copied().unwrap_or(0.0)),
                ),
                throughput_change_percent: None,
                rss_change_percent: None,
            }
        })
        .collect::<Vec<_>>();

    let mut saturation = None;
    for index in 1..results.len() {
        let throughput_change =
            percent_change(results[index - 1].items_per_s, results[index].items_per_s);
        let rss_change = percent_change(results[index - 1].peak_rss_mb, results[index].peak_rss_mb);
        results[index].throughput_change_percent = Some(throughput_change);
        results[index].rss_change_percent = Some(rss_change);
        if saturation.is_none() && (throughput_change < 5.0 || rss_change > 25.0) {
            saturation = Some(Saturation {
                concurrency: results[index].concurrency,
                reason: if throughput_change < 5.0 {
                    "throughput_gain_below_5_percent"
                } else {
                    "rss_growth_above_25_percent"
                },
            });
        }
    }

    ScaleReport {
        runs_per_level: RUNS_PER_LEVEL,
        results,
        saturation,
    }
}

fn median(values: impl Iterator<Item = f64>) -> f64 {
    let mut values = values.collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn percent_change(before: f64, after: f64) -> f64 {
    if before == 0.0 {
        0.0
    } else {
        (after - before) / before * 100.0
    }
}

fn render_markdown(report: &ScaleReport) -> String {
    let mut lines = vec![
        "# Kumo Scaling Benchmark".to_string(),
        String::new(),
        "| Concurrency | Peak in flight | Items/s | Elapsed (s) | RSS (MB) | Fetch (s) | Parse (s) |".to_string(),
        "|---:|---:|---:|---:|---:|---:|---:|".to_string(),
    ];
    for result in &report.results {
        lines.push(format!(
            "| {} | {} | {:.1} | {:.3} | {:.1} | {:.3} | {:.3} |",
            result.concurrency,
            result.peak_in_flight,
            result.items_per_s,
            result.elapsed_s,
            result.peak_rss_mb,
            result.fetch_secs,
            result.parse_secs
        ));
    }
    lines.extend([
        String::new(),
        report.saturation.as_ref().map_or_else(
            || "Saturation: not observed".to_string(),
            |saturation| {
                format!(
                    "Saturation: concurrency {} ({})",
                    saturation.concurrency, saturation.reason
                )
            },
        ),
        String::new(),
    ]);
    lines.join("\n")
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

    fn raw(elapsed_s: f64, rss_kb: f64, peak_in_flight: usize) -> RawResult {
        RawResult {
            elapsed_s,
            items: 1_000,
            pages: 50,
            peak_rss_kb: rss_kb,
            peak_in_flight,
            timings: BTreeMap::from([
                ("fetch_secs".to_string(), elapsed_s / 2.0),
                ("parse_secs".to_string(), elapsed_s / 4.0),
            ]),
        }
    }

    #[test]
    fn reports_medians_and_first_saturation_point() {
        let grouped = vec![
            (
                1,
                vec![
                    raw(1.1, 10_240.0, 1),
                    raw(1.0, 10_240.0, 1),
                    raw(0.9, 10_240.0, 1),
                ],
            ),
            (
                4,
                vec![
                    raw(0.3, 11_264.0, 4),
                    raw(0.25, 11_264.0, 4),
                    raw(0.2, 11_264.0, 4),
                ],
            ),
            (
                8,
                vec![
                    raw(0.24, 11_264.0, 8),
                    raw(0.24, 11_264.0, 8),
                    raw(0.23, 11_264.0, 8),
                ],
            ),
        ];

        let report = summarize(&grouped);

        assert_eq!(report.results[0].items_per_s, 1_000.0);
        assert_eq!(report.results[1].peak_in_flight, 4);
        assert_eq!(report.saturation.as_ref().unwrap().concurrency, 8);
        assert!(render_markdown(&report).contains("Peak in flight"));
    }
}
