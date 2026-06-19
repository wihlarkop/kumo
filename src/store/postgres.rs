use super::ItemStore;
use crate::error::KumoError;
use async_trait::async_trait;
use sqlx::{AssertSqlSafe, PgPool};

pub struct PostgresStore {
    pool: PgPool,
    table: String,
    extra_columns: Vec<String>,
}

pub struct PostgresStoreBuilder {
    database_url: String,
    table: String,
    create_table: bool,
    extra_columns: Vec<(String, String)>,
}

impl PostgresStore {
    /// Connect and create the default table `kumo_items` if it does not exist.
    pub async fn connect(database_url: &str) -> Result<Self, KumoError> {
        Self::builder(database_url).connect().await
    }

    /// Builder for a custom table name, extra columns, or to skip auto-create.
    pub fn builder(database_url: impl Into<String>) -> PostgresStoreBuilder {
        PostgresStoreBuilder {
            database_url: database_url.into(),
            table: "kumo_items".into(),
            create_table: true,
            extra_columns: Vec::new(),
        }
    }

    fn insert_sql(&self) -> String {
        let col_list: String = self
            .extra_columns
            .iter()
            .map(|n| format!(", \"{}\"", n))
            .collect();
        let param_list: String = (2..=self.extra_columns.len() + 1)
            .map(|i| format!(", ${}", i))
            .collect();
        format!(
            r#"INSERT INTO "{}" (data{}) VALUES ($1{})"#,
            self.table, col_list, param_list
        )
    }

    fn batch_insert_sql(&self, rows: usize) -> String {
        let col_list: String = self
            .extra_columns
            .iter()
            .map(|n| format!(", \"{}\"", n))
            .collect();
        let columns_per_row = self.extra_columns.len() + 1;
        let values = (0..rows)
            .map(|row| {
                let params = (1..=columns_per_row)
                    .map(|offset| format!("${}", row * columns_per_row + offset))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", params)
            })
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            r#"INSERT INTO "{}" (data{}) VALUES {}"#,
            self.table, col_list, values
        )
    }
}

impl PostgresStoreBuilder {
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
    /// `sql_type` is any valid Postgres type (`TEXT`, `INT`, `JSONB`, etc.).
    /// The value is taken from the JSON field whose key matches `name`; missing
    /// fields are stored as NULL.
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
    pub async fn connect(self) -> Result<PostgresStore, KumoError> {
        super::validate_table_name(&self.table)?;

        let pool = PgPool::connect(&self.database_url)
            .await
            .map_err(|e| KumoError::store("postgres store", e))?;

        if self.create_table {
            let extra = self
                .extra_columns
                .iter()
                .map(|(name, ty)| format!(",\n                    \"{}\" {}", name, ty))
                .collect::<String>();
            let sql = format!(
                r#"CREATE TABLE IF NOT EXISTS "{}" (
                    id         BIGSERIAL PRIMARY KEY,
                    data       JSONB NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(){}
                )"#,
                self.table, extra
            );
            sqlx::query(AssertSqlSafe(sql))
                .execute(&pool)
                .await
                .map_err(|e| KumoError::store("postgres store", e))?;
        }

        Ok(PostgresStore {
            pool,
            table: self.table,
            extra_columns: self.extra_columns.into_iter().map(|(n, _)| n).collect(),
        })
    }
}

#[async_trait]
impl ItemStore for PostgresStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let sql = self.insert_sql();
        let mut q = sqlx::query(AssertSqlSafe(sql)).bind(item);
        for name in &self.extra_columns {
            q = q.bind(super::json_val_to_sql_string(item.get(name)));
        }
        q.execute(&self.pool)
            .await
            .map_err(|e| KumoError::store("postgres store", e))?;
        Ok(())
    }

    async fn store_many(&self, items: &[serde_json::Value]) -> Result<(), KumoError> {
        if items.is_empty() {
            return Ok(());
        }

        let sql = self.batch_insert_sql(items.len());
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| KumoError::store("postgres store", e))?;
        let mut q = sqlx::query(AssertSqlSafe(sql));
        for item in items {
            q = q.bind(item);
            for name in &self.extra_columns {
                q = q.bind(super::json_val_to_sql_string(item.get(name)));
            }
        }
        q.execute(&mut *tx)
            .await
            .map_err(|e| KumoError::store("postgres store", e))?;
        tx.commit()
            .await
            .map_err(|e| KumoError::store("postgres store", e))?;
        Ok(())
    }
}
