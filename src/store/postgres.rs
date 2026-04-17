use super::ItemStore;
use crate::error::KumoError;
use async_trait::async_trait;
use sqlx::PgPool;

pub struct PostgresStore {
    pool: PgPool,
    table: String,
}

pub struct PostgresStoreBuilder {
    database_url: String,
    table: String,
    create_table: bool,
}

impl PostgresStore {
    /// Connect and create the default table `kumo_items` if it does not exist.
    pub async fn connect(database_url: &str) -> Result<Self, KumoError> {
        Self::builder(database_url).connect().await
    }

    /// Builder for a custom table name or to skip auto-create.
    pub fn builder(database_url: impl Into<String>) -> PostgresStoreBuilder {
        PostgresStoreBuilder {
            database_url: database_url.into(),
            table: "kumo_items".into(),
            create_table: true,
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

    /// Validate the table name, connect, optionally create the table, return the store.
    pub async fn connect(self) -> Result<PostgresStore, KumoError> {
        super::validate_table_name(&self.table)?;

        let pool = PgPool::connect(&self.database_url)
            .await
            .map_err(|e| KumoError::Store(e.to_string()))?;

        if self.create_table {
            let sql = format!(
                r#"CREATE TABLE IF NOT EXISTS "{}" (
                    id         BIGSERIAL PRIMARY KEY,
                    data       JSONB NOT NULL,
                    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
                )"#,
                self.table
            );
            sqlx::query(&sql)
                .execute(&pool)
                .await
                .map_err(|e| KumoError::Store(e.to_string()))?;
        }

        Ok(PostgresStore {
            pool,
            table: self.table,
        })
    }
}

#[async_trait]
impl ItemStore for PostgresStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let sql = format!(r#"INSERT INTO "{}" (data) VALUES ($1)"#, self.table);
        sqlx::query(&sql)
            .bind(item)
            .execute(&self.pool)
            .await
            .map_err(|e| KumoError::Store(e.to_string()))?;
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
