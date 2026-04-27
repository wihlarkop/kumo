#[cfg(feature = "cloud")]
pub mod cloud;
pub mod csv;
pub mod json;
pub mod jsonl;
#[cfg(feature = "mysql")]
pub mod mysql;
#[cfg(feature = "postgres")]
pub mod postgres;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod stdout;

#[cfg(feature = "cloud")]
pub use cloud::{CloudFormat, CloudStore, CloudStoreBuilder};
pub use csv::CsvStore;
pub use json::JsonStore;
pub use jsonl::JsonlStore;
#[cfg(feature = "mysql")]
pub use mysql::{MySqlStore, MySqlStoreBuilder};
#[cfg(feature = "postgres")]
pub use postgres::{PostgresStore, PostgresStoreBuilder};
#[cfg(feature = "sqlite")]
pub use sqlite::{SqliteStore, SqliteStoreBuilder};
pub use stdout::StdoutStore;

use crate::error::KumoError;

/// Pluggable output backend for scraped items.
///
/// Items arrive pre-serialized as `serde_json::Value` from `Output::item()`.
/// Implement this trait to send scraped data to a database, Kafka, S3, etc.
#[async_trait::async_trait]
pub trait ItemStore: Send + Sync {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError>;

    /// Flush any buffered writes. Called by the engine after the crawl finishes.
    async fn flush(&self) -> Result<(), KumoError> {
        Ok(())
    }
}

/// Validate that a table/collection name is safe to interpolate into SQL.
/// Accepts only ASCII alphanumeric characters and underscores, 1–63 characters.
#[cfg(any(feature = "postgres", feature = "sqlite", feature = "mysql"))]
pub(super) fn validate_table_name(name: &str) -> Result<(), crate::error::KumoError> {
    if name.is_empty() || name.len() > 63 {
        return Err(crate::error::KumoError::store_msg(format!(
            "table name must be 1–63 characters, got {}",
            name.len()
        )));
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(crate::error::KumoError::store_msg(format!(
            "table name '{}' contains invalid characters (only a-z, A-Z, 0-9, _ allowed)",
            name
        )));
    }
    Ok(())
}

/// Convert an optional JSON value to the `Option<String>` form expected by sqlx bindings.
/// Null JSON values map to SQL NULL; strings are used as-is; everything else is serialized.
#[cfg(any(feature = "postgres", feature = "sqlite", feature = "mysql"))]
pub(super) fn json_val_to_sql_string(val: Option<&serde_json::Value>) -> Option<String> {
    val.and_then(|v| {
        if v.is_null() {
            None
        } else if let Some(s) = v.as_str() {
            Some(s.to_string())
        } else {
            Some(v.to_string())
        }
    })
}
