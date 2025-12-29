use axum::{Extension, Json, extract::State};
use reqwest::{Client, StatusCode};
use tracing::instrument;

use crate::{
    extractors::LoggedIn,
    facebook_graph_api::page_api::{FacebookPages, get_facebook_pages},
    server::AppState,
};

#[axum::debug_handler]
#[instrument(skip(client, auth))]
pub async fn facebooks_pages(
    State(_): State<AppState>,
    Extension(client): Extension<Client>,
    LoggedIn(auth): LoggedIn,
) -> Result<Json<FacebookPages>, StatusCode> {
    let fb_pages = get_facebook_pages(&client, "24.0", &auth.user_id, &auth.user_access_token)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "page credentials failed");
            StatusCode::BAD_GATEWAY
        })?;

    Ok(Json(fb_pages))
}
