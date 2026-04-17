use super::ItemStore;
use crate::error::KumoError;
use async_trait::async_trait;
use sqlx::MySqlPool;

pub struct MySqlStore {
    pool: MySqlPool,
    table: String,
}

pub struct MySqlStoreBuilder {
    database_url: String,
    table: String,
    create_table: bool,
}

impl MySqlStore {
    /// Connect to a MySQL/MariaDB database and create the default table `kumo_items` if missing.
    ///
    /// Use `mysql://user:pass@host/db`.
    pub async fn connect(database_url: &str) -> Result<Self, KumoError> {
        Self::builder(database_url).connect().await
    }

    /// Builder for a custom table name or to skip auto-create.
    pub fn builder(database_url: impl Into<String>) -> MySqlStoreBuilder {
        MySqlStoreBuilder {
            database_url: database_url.into(),
            table: "kumo_items".into(),
            create_table: true,
        }
    }
}

impl MySqlStoreBuilder {
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
    pub async fn connect(self) -> Result<MySqlStore, KumoError> {
        super::validate_table_name(&self.table)?;

        let pool = MySqlPool::connect(&self.database_url)
            .await
            .map_err(|e| KumoError::Store(e.to_string()))?;

        if self.create_table {
            let sql = format!(
                r#"CREATE TABLE IF NOT EXISTS `{}` (
                    id         BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
                    data       JSON NOT NULL,
                    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
                )"#,
                self.table
            );
            sqlx::query(&sql)
                .execute(&pool)
                .await
                .map_err(|e| KumoError::Store(e.to_string()))?;
        }

        Ok(MySqlStore {
            pool,
            table: self.table,
        })
    }
}

#[async_trait]
impl ItemStore for MySqlStore {
    async fn store(&self, item: &serde_json::Value) -> Result<(), KumoError> {
        let sql = format!(r#"INSERT INTO `{}` (data) VALUES (?)"#, self.table);
        sqlx::query(&sql)
            .bind(item.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| KumoError::Store(e.to_string()))?;
        Ok(())
    }
}
