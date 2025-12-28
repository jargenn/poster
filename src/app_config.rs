use camino::Utf8PathBuf;
use eyre::Result;
use secrecy::SecretString;
use serde::Deserialize;

use crate::facebook_graph_api::auth::RedirectUri;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub app_id: String,
    pub app_secret: SecretString,
    pub redirect_uri: RedirectUri,
    pub fb_config_id: SecretString,
    pub database_path: SecretString,
}

impl AppConfig {
    pub fn from_config(config_file: Utf8PathBuf) -> Result<Self> {
        let settings = config::Config::builder()
            .add_source(config::File::from(config_file.as_std_path()).required(true))
            .build()?;

        Ok(settings.try_deserialize()?)
    }
}
