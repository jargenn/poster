use eyre::Result;
use facebook_graph_api::auth::RedirectUri;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

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

#[derive(Debug, Deserialize, Clone)]
pub struct CacheSettings {
    pub name: String,
    pub ttl: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub host: String,
    pub port: u16,
    pub database: DatabaseSettings,
    #[serde(rename = "cache")]
    pub cache_settings: CacheSettings,
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

#[derive(Debug, Deserialize, Clone)]
pub struct ConfigData {
    #[serde(rename = "id")]
    pub app_id: String,
    #[serde(rename = "secret")]
    pub app_secret: String,
    #[serde(rename = "config_id")]
    pub app_config_id: String,
    pub redirect_url: RedirectUri,
}

#[derive(Deserialize)]
pub struct UserConfig {
    #[serde(flatten)]
    pub config_data: ConfigData,
    pub description: Option<String>,
}
