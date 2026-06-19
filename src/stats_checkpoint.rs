use std::{fs, path::PathBuf, time::Duration};

use crate::{
    error::KumoError,
    stats::{CrawlReport, CrawlStats},
};

const DEFAULT_INTERVAL: Duration = Duration::from_secs(30);

/// Configuration for writing crawl report checkpoints during a crawl.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatsCheckpointConfig {
    path: PathBuf,
    interval: Duration,
}

impl StatsCheckpointConfig {
    /// Write checkpoints to `path` every 30 seconds.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            interval: DEFAULT_INTERVAL,
        }
    }

    /// Write checkpoints to `path` at a custom interval.
    pub fn with_interval(path: impl Into<PathBuf>, interval: Duration) -> Self {
        Self {
            path: path.into(),
            interval,
        }
    }

    /// Checkpoint file path.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// Checkpoint write interval.
    pub fn interval(&self) -> Duration {
        self.interval
    }
}

pub(crate) async fn write_stats_checkpoint(
    config: &StatsCheckpointConfig,
    stats: CrawlStats,
) -> Result<(), KumoError> {
    write_json(config, CrawlReport::from(stats).to_json_value()).await
}

pub(crate) async fn write_stats_checkpoints(
    config: &StatsCheckpointConfig,
    stats: Vec<CrawlStats>,
) -> Result<(), KumoError> {
    let reports = stats
        .into_iter()
        .map(|stats| CrawlReport::from(stats).to_json_value())
        .collect::<Vec<_>>();
    write_json(config, serde_json::Value::Array(reports)).await
}

async fn write_json(
    config: &StatsCheckpointConfig,
    value: serde_json::Value,
) -> Result<(), KumoError> {
    if let Some(parent) = config.path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| KumoError::store(format!("create {}", parent.display()), e))?;
    }

    let json = serde_json::to_string_pretty(&value)
        .map_err(|e| KumoError::store("serialize stats checkpoint", e))?;
    let tmp_path = config.path.with_extension(format!(
        "{}.tmp",
        config
            .path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));

    fs::write(&tmp_path, json)
        .map_err(|e| KumoError::store(format!("write {}", tmp_path.display()), e))?;
    fs::rename(&tmp_path, &config.path)
        .map_err(|e| KumoError::store(format!("rename {}", config.path.display()), e))
}
