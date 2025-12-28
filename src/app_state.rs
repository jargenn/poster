use std::path::PathBuf;

use reqwest::Client;

use crate::login::RedirectUri;

const APP_ID: &str = "1408225520952821";
const APP_SECRET: &str = "5264b009d76aaa3ed19f60767c349c3d";
const CONFIGURATION_ID: &str = "25537253695887249";

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_id: String,
    pub app_secret: String,
    pub app_access_token: String,
    pub redirect_uri: RedirectUri,
    pub fb_config_id: String,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub client: Client,
    pub config: AppConfig,
    pub db: PathBuf,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            config: AppConfig {
                app_id: APP_ID.to_owned(),
                app_secret: APP_SECRET.to_owned(),
                fb_config_id: CONFIGURATION_ID.to_owned(),
                app_access_token: format!("{APP_ID}|{APP_SECRET}"),
                redirect_uri: RedirectUri::default(),
            },
            client: Client::new(),
            db: PathBuf::from("store.db"),
        }
    }
}
