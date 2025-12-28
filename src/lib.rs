use eyre::Result;
use serde::{Deserialize, Serialize};
use tracing::instrument;

pub mod cookies;
pub mod db;
#[cfg(debug_assertions)]
pub mod debug;
pub mod extractors;
pub mod flows;
pub mod server;
pub mod telemetry;

pub use flows::*;

pub fn get_page_id_url(version: String, user_id: usize, user_access_token: String) -> String {
    format!(
        "https://graph.facebook.com/v{version}/{user_id}/accounts?access_token={user_access_token}"
    )
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Task {
    #[default]
    Analyze,
    Advertise,
    Moderate,
    CreateContent,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Category {
    id: usize,
    name: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Page {
    /// Short-lived access token
    /// TODO: Search for how short-lived it is
    access_token: String,
    category: String,
    category_list: Vec<Category>,
    name: String,
    id: usize,
    tasks: Vec<Task>,
}

/// List of IDs and Page access tokens for pages on which I can perform a [`Task`].
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct FacebookPages {
    pub data: Vec<Page>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct PageCredentials {
    pub page_id: String,
    pub page_access_token: usize,
}

#[instrument("Retrieving credentials for the Page...")]
pub async fn get_page_credentials() -> Result<PageCredentials> {
    // 1. Get a User Access Token from your app user through Facebook Login for Business.
    tracing::info!("Requesting a User Acess Token for Poster...");
    // 2. Query the /me/accounts endpoint to get the ID and Page Access Token of the Page the app User has permitted your app to access.
    tracing::info!("Requesting a the credentials of the pages using the  User Acess Token...");
    let _pages = FacebookPages {
        data: vec![Page {
            category_list: vec![Category::default()],
            tasks: vec![Task::default()],
            ..Default::default()
        }],
    };

    //3 Capture the returned Page ID and Page Access Token.
    tracing::info!("Credentials captured!");
    todo!();
}
