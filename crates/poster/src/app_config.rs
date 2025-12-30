use eyre::Result;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

use facebook_graph_api::auth::RedirectUri;

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub username: String,
    pub password: SecretString,
    pub port: u16,
    pub host: String,
    pub database_name: String,
}

impl DatabaseSettings {
    pub fn connection_string(&self) -> String {
        format!(
            "postgres://{}:{}@{}:{}/{}",
            self.username,
            self.password.expose_secret(),
            self.host,
            self.port,
            self.database_name
        )
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub app_id: String,
    pub app_secret: SecretString,
    pub redirect_uri: RedirectUri,
    pub fb_config_id: SecretString,
    pub database: DatabaseSettings,
}

impl AppConfig {
    pub fn get_config() -> Result<Self> {
        let base_path = std::env::current_dir().expect("Failed to determine current directory");
        let config_dir = base_path.join("configuration");
        let env = std::env::var("APP_ENVIRONMENT").unwrap_or_else(|_| "local".into());

        let settings = config::Config::builder()
            .add_source(config::File::from(config_dir.join("base")).required(true))
            .add_source(config::File::from(config_dir.join(env.as_str())).required(true))
            .add_source(config::Environment::with_prefix("app").separator("__"))
            .build()?;

        Ok(settings.try_deserialize()?)
    }
}
