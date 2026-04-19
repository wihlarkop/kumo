use super::ItemStore;
use crate::error::KumoError;
use async_trait::async_trait;
use sqlx::SqlitePool;

pub struct SqliteStore {
    pool: SqlitePool,
    table: String,
    extra_columns: Vec<String>,
}

pub struct SqliteStoreBuilder {
    database_url: String,
    table: String,
    create_table: bool,
    extra_columns: Vec<(String, String)>,
}

impl SqliteStore {
    /// Connect to a SQLite database and create the default table `kumo_items` if missing.
    ///
    /// Use `sqlite://path/to/db.sqlite` or `sqlite::memory:` for an in-memory database.
    pub async fn connect(database_url: &str) -> Result<Self, KumoError> {
        Self::builder(database_url).connect().await
    }

    /// Builder for a custom table name, extra columns, or to skip auto-create.
    pub fn builder(database_url: impl Into<String>) -> SqliteStoreBuilder {
        SqliteStoreBuilder {
            database_url: database_url.into(),
            table: "kumo_items".into(),
            create_table: true,
            extra_columns: Vec::new(),
        }
    }
}

impl SqliteStoreBuilder {
    /// Override the table name (default: `kumo_items`).
    pub fn table(mut self, name: impl Into<String>) -> Self {
        self.table = name.into();
        self
    }

    /// Whether to CREATE TABLE IF NOT EXISTS on connect (default: true).
    pub fn create_table(mut self, yes: bool) -> Self {
        self.create_table = yes;
        self
    }

    /// Add an extra column extracted from the scraped JSON by matching key name.
    ///
    /// `sql_type` is any valid SQLite type affinity (`TEXT`, `INTEGER`, `REAL`, etc.).
    /// Missing fields are stored as NULL.
    pub fn add_column(
        mut self,
        name: impl Into<String>,
        sql_type: impl Into<String>,
    ) -> Result<Self, KumoError> {
        let name = name.into();
        super::validate_table_name(&name)?;
        self.extra_columns.push((name, sql_type.into()));
        Ok(self)
    }

    /// Validate the table name, connect, optionally create the table, return the store.
    pub async fn connect(self) -> Result<SqliteStore, KumoError> {
        super::validate_table_name(&self.table)?;

        let pool = SqlitePool::connect(&self.database_url)
            .await
            .map_err(|e| KumoError::store("sqlite store", e))?;

        if self.create_table {
            let extra = self
                .extra_columns
                .iter()
                .map(|(name, ty)| format!(",\n                    \"{}\" {}", name, ty))
                .collect::<String>();
            let sql = format!(
                r#"CREATE TABLE IF NOT EXISTS "{}" (
                    id         INTEGER PRIMARY KEY AUTOINCREMENT,
                    data       TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (datetime('now')){}
                )"#,
                self.table, extra
            );
            sqlx::query(&sql)
                .execute(&pool)
                .await
                .map_err(|e| KumoError::store("sqlite store", e))?;
        }

        Ok(SqliteStore {
            pool,
            table: self.table,
            extra_columns: self.extra_columns.into_iter().map(|(n, _)| n).collect(),
        })
    }
}

#[async_trait]
impl ItemStore for SqliteStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let col_list: String = self
            .extra_columns
            .iter()
            .map(|n| format!(", \"{}\"", n))
            .collect();
        let param_list: String = self.extra_columns.iter().map(|_| ", ?").collect();
        let sql = format!(
            r#"INSERT INTO "{}" (data{}) VALUES (?{})"#,
            self.table, col_list, param_list
        );
        let mut q = sqlx::query(&sql).bind(item.to_string());
        for name in &self.extra_columns {
            let val = item.get(name);
            let bound: Option<String> = val.and_then(|v| {
                if v.is_null() {
                    None
                } else if let Some(s) = v.as_str() {
                    Some(s.to_string())
                } else {
                    Some(v.to_string())
                }
            });
            q = q.bind(bound);
        }
        q.execute(&self.pool)
            .await
            .map_err(|e| KumoError::store("sqlite store", e))?;
        Ok(())
    }
}
