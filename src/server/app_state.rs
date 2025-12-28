use camino::Utf8PathBuf;
use eyre::Result;
use secrecy::SecretString;
use serde::Deserialize;

use crate::auth::RedirectUri;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub app_id: String,
    pub app_secret: SecretString,
    pub redirect_uri: RedirectUri,
    pub fb_config_id: SecretString,
    pub database_path: Utf8PathBuf,
}

impl AppConfig {
    pub fn from_config(config_file: Utf8PathBuf) -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::from(config_file.as_std_path()).required(true))
            .build()?;

        Ok(settings.try_deserialize()?)
    }
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: AppConfig,
}

impl AppState {
    pub fn new() -> Result<Self> {
        let config_file = std::env::var("CONFIG_FILE").unwrap_or_else(|_| "config".to_owned());
        Ok(Self {
            config: AppConfig::from_config(config_file.into())?,
        })
    }
}
