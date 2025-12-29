use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;

use crate::Error;
use crate::GraphApiError;

fn page_access_token_endpoint(version: &str, user_id: &str, user_access_token: &str) -> String {
    // curl -i -X GET "https://graph.facebook.com/{your-user-id}/accounts?access_token={user-access-token}"
    url::Url::parse_with_params(
        &format!("https://graph.facebook.com/v{version}/{user_id}/accounts"),
        &[("access_token", user_access_token)],
    )
    .expect("Failed to parse page access token endpoint url")
    .to_string()
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Task {
    Analyze,
    Advertise,
    Manage,
    Moderate,
    Messaging,
    CreateContent,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Category {
    id: String,
    name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Page {
    /// Short-lived access token
    /// TODO: Search for how short-lived it is
    access_token: String,
    category: String,
    category_list: Vec<Category>,
    name: String,
    id: String,
    tasks: Vec<Task>,
}

/// List of IDs and Page access tokens for pages on which I can perform a [`Task`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FacebookPages {
    pub data: Vec<Page>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PageCredentials {
    pub page_id: String,
    pub page_access_token: usize,
}

pub async fn get_facebook_pages(
    client: &Client,
    version: &str,
    user_id: &str,
    user_access_token: &str,
) -> Result<FacebookPages, Error> {
    tracing::info!(%user_id, "requesting page credentials");

    let endpoint = page_access_token_endpoint(version, user_id, user_access_token);
    debug!(%endpoint, "The endpoint used");

    let res = client.get(endpoint).send().await?;

    let status = res.status();

    let text = res.text().await?;
    debug!("{}", text);

    if !status.is_success() {
        let graph_error = GraphApiError::from_response_body(&text)?;

        tracing::warn!(
            %user_id,
            fb_code = %graph_error.code,
            trace_id = %graph_error.trace_id,
            "facebook rejected request"
        );

        return Err(graph_error)?;
    }

    let credentials = serde_json::from_str::<FacebookPages>(&text)?;

    Ok(credentials)
}
