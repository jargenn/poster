use serde::Deserialize;
use sqlx::{ConnectOptions, sqlite::SqliteConnectOptions};
use std::str::FromStr;

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub url: String,
}

impl DatabaseSettings {
    pub fn with_db(&self) -> SqliteConnectOptions {
        SqliteConnectOptions::from_str(&self.url)
            .expect("Invalid SQLite database URL")
            .create_if_missing(true)
            .foreign_keys(true)
            .log_statements(tracing::log::LevelFilter::Trace)
    }
}
