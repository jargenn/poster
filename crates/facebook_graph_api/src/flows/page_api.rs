use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use tracing::debug;

use crate::Error;
use crate::GraphApiError;

fn page_access_token_endpoint(version: &str, user_id: &str, user_access_token: &str) -> String {
    url::Url::parse_with_params(
        &format!("https://graph.facebook.com/v{version}/{user_id}/accounts"),
        &[("access_token", user_access_token)],
    )
    .expect("Failed to parse page access token endpoint url")
    .to_string()
}

fn post_to_page_endpoint(version: &str, page_id: &str) -> String {
    url::Url::parse(&format!(
        "https://graph.facebook.com/v{version}/{page_id}/feed"
    ))
    .expect("Failed to parse page post endpoint url")
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
    ViewMonetizationInsights,
    ManageLeads,
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
    pub access_token: String,
    pub category: String,
    pub category_list: Vec<Category>,
    pub name: String,
    pub id: String,
    pub tasks: Vec<Task>,
}

/// List of IDs and Page access tokens for pages on which I can perform a [`Task`].
#[derive(Debug, Serialize, Deserialize)]
pub struct FacebookPages {
    pub data: Vec<Page>,
}

pub async fn get_facebook_pages(
    client: &Client,
    version: &str,
    user_id: &str,
    user_access_token: &str,
) -> Result<FacebookPages, Error> {
    tracing::info!(%user_id, "requesting pages information of the user");

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

    let pages = serde_json::from_str::<FacebookPages>(&text)?;

    Ok(pages)
}

pub async fn get_page_credentials(
    client: &Client,
    version: &str,
    page_id: &str,
    user_id: &str,
    user_access_token: &str,
) -> Result<Page, Error> {
    tracing::info!(%user_id, "requesting page credentials");
    let fb_pages = get_facebook_pages(client, version, user_id, user_access_token).await?;

    let page = fb_pages.data.into_iter().find(|page| page.id == page_id);

    match page {
        Some(p) => Ok(p),
        None => Err(Error::PageNotFound {
            page_id: page_id.to_string(),
            user_id: user_id.to_string(),
        })?,
    }
}

pub async fn post_to_page(
    client: &Client,
    version: &str,
    message: &str,
    page_id: &str,
    page_access_token: &str,
) -> Result<String, Error> {
    let endpoint = post_to_page_endpoint(version, page_id);
    debug!(%endpoint, "The endpoint used");

    let payload = json!({
        "message":message,
        "access_token":page_access_token,
    });

    debug!("Payload sent {}", payload);
    let res = client.post(endpoint).json(&payload).send().await?;

    let status = res.status();
    let text = res.text().await?;
    debug!("{}", text);

    if !status.is_success() {
        let graph_error = GraphApiError::from_response_body(&text)?;
        tracing::warn!(
            %page_id,
            "facebook rejected request"
        );

        return Err(graph_error)?;
    }

    #[derive(Deserialize)]
    struct PostSuccess {
        id: String,
    }

    let post_id = serde_json::from_str::<PostSuccess>(&text)?;

    Ok(post_id.id)
}
