use color_eyre::owo_colors::OwoColorize;
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use tracing::debug;
use tracing::instrument;
use url::Url;

use crate::Error;
use crate::FacebookPost;
use crate::GraphApiError;

fn page_access_token_endpoint(
    version: &str,
    user_id: &str,
    user_access_token: &str,
    facebook_uri: Url,
) -> String {
    let url = facebook_uri
        .join(&format!("v{version}/{user_id}/accounts"))
        .expect("Failed to construct a URL")
        .to_string();

    url::Url::parse_with_params(&url, &[("access_token", user_access_token)])
        .expect("Failed to parse page access token endpoint url")
        .to_string()
}

fn feed_endpoint(version: &str, page_id: &str, facebook_uri: Url) -> String {
    facebook_uri
        .join(&format!("v{version}/{page_id}/feed"))
        .expect("Failed to parse page post endpoint url")
        .to_string()
}

fn photos_endpoint(version: &str, page_id: &str, facebook_uri: Url) -> String {
    facebook_uri
        .join(&format!("v{version}/{page_id}/photos"))
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
    facebook_uri: Url,
) -> Result<FacebookPages, Error> {
    tracing::info!(%user_id, "requesting pages information of the user");

    let endpoint = page_access_token_endpoint(version, user_id, user_access_token, facebook_uri);
    debug!(%endpoint, "The endpoint used");

    let res = client.get(endpoint).send().await?;

    let status = res.status();

    let text = res.text().await?;
    debug!(body = text, "Response Body");

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
    facebook_uri: Url,
) -> Result<Page, Error> {
    tracing::info!(%user_id, "requesting page credentials");
    let fb_pages =
        get_facebook_pages(client, version, user_id, user_access_token, facebook_uri).await?;

    let page = fb_pages.data.into_iter().find(|page| page.id == page_id);

    match page {
        Some(p) => Ok(p),
        None => Err(Error::PageNotFound {
            page_id: page_id.to_string(),
            user_id: user_id.to_string(),
        })?,
    }
}

// #[derive(Debug)]
// pub struct FacebookPost {
//     pub message: String,
//     pub published: bool,
//     pub link: Option<String>,
//     pub scheduled_publish_time: Option<ScheduledTime>,
//     pub media_url: Option<Vec<Input>>,
// }

#[instrument("Posting debug!", skip(client, version), fields(
        user_access_token=%user_access_token.bold(),
        user_id=%user_id.bold(),
        page_id=%page_id.bold(),
))]
pub async fn post_to_page(
    client: &Client,
    version: &str,
    post: FacebookPost,
    page_id: &str,
    user_access_token: &str,
    user_id: &str,
    facebook_uri: Url,
) -> Result<String, Error> {
    let page_credential = get_page_credentials(
        client,
        version,
        page_id,
        user_id,
        user_access_token,
        facebook_uri.clone(),
    )
    .await?;

    let page_access_token = page_credential.access_token;

    // TODO: Work on this
    let endpoint = if post.media_url.is_none() {
        feed_endpoint(version, page_id, facebook_uri)
    } else {
        photos_endpoint(version, page_id, facebook_uri)
    };

    debug!(%endpoint, "The endpoint used");

    let payload = post.to_payload(page_access_token)?;
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

#[derive(Serialize, Deserialize, Debug)]
pub struct PostData {
    pub created_time: String,
    pub message: String,
    #[serde(rename = "id")]
    pub page_post_id: String,
}

pub async fn get_page_posts(
    client: &Client,
    version: &str,
    page_id: &str,
    page_access_token: &str,
    facebook_uri: Url,
) -> Result<PostData, Error> {
    let endpoint = format!(
        "{}?access_token={page_access_token}",
        feed_endpoint(version, page_id, facebook_uri)
    );
    debug!(%endpoint, "The endpoint used");

    let res = client.get(endpoint).send().await?;

    let status = res.status();
    let text = res.text().await?;
    debug!(body = text, "Response Body");

    if !status.is_success() {
        let graph_error = GraphApiError::from_response_body(&text)?;
        tracing::warn!(
            %page_id,
            "facebook rejected request"
        );

        return Err(graph_error)?;
    }

    Ok(serde_json::from_str::<PostData>(&text)?)
}
