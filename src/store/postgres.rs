use super::ItemStore;
use crate::error::KumoError;
use async_trait::async_trait;
use sqlx::PgPool;

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
            sqlx::query(&sql)
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
        let col_list: String = self
            .extra_columns
            .iter()
            .map(|n| format!(", \"{}\"", n))
            .collect();
        let param_list: String = (2..=self.extra_columns.len() + 1)
            .map(|i| format!(", ${}", i))
            .collect();
        let sql = format!(
            r#"INSERT INTO "{}" (data{}) VALUES ($1{})"#,
            self.table, col_list, param_list
        );
        let mut q = sqlx::query(&sql).bind(item);
        for name in &self.extra_columns {
            q = q.bind(super::json_val_to_sql_string(item.get(name)));
        }
        q.execute(&self.pool)
            .await
            .map_err(|e| KumoError::store("postgres store", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn valid_table_names_are_accepted() {
        assert!(super::super::validate_table_name("kumo_items").is_ok());
        assert!(super::super::validate_table_name("items").is_ok());
        assert!(super::super::validate_table_name("my_table_123").is_ok());
        assert!(super::super::validate_table_name("A").is_ok());
    }

    #[test]
    fn empty_table_name_is_rejected() {
        assert!(super::super::validate_table_name("").is_err());
    }

    #[test]
    fn table_name_over_63_chars_is_rejected() {
        let long = "a".repeat(64);
        assert!(super::super::validate_table_name(&long).is_err());
    }

    #[test]
    fn table_name_with_sql_injection_is_rejected() {
        assert!(super::super::validate_table_name("drop table;--").is_err());
        assert!(super::super::validate_table_name("items; DROP TABLE users;--").is_err());
        assert!(super::super::validate_table_name("my-table").is_err());
        assert!(super::super::validate_table_name("my table").is_err());
    }
}
