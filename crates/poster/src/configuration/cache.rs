use facebook_graph_api::auth::RedirectUri;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct CacheSettings {
    pub name: String,
    pub ttl: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CacheStore {
    pub user_config: CacheSettings,
    pub auth_data: CacheSettings,
}

#[derive(Debug, Deserialize, Clone)]
pub struct FbConfig {
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
    pub config_data: FbConfig,
    pub description: Option<String>,
}
