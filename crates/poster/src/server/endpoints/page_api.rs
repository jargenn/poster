use axum::{
    Extension, Json,
    extract::{Path, State},
};
use reqwest::Client;
use tracing::instrument;

use crate::{extractors::LoggedIn, server::AppState};
use facebook_graph_api::{
    Error,
    page_api::{FacebookPages, PageCredentials, get_facebook_pages, get_page_credentials},
};

#[instrument(skip(client, auth))]
pub async fn facebooks_pages(
    State(_): State<AppState>,
    Extension(client): Extension<Client>,
    LoggedIn(auth): LoggedIn,
) -> Result<Json<FacebookPages>, Error> {
    let fb_pages =
        get_facebook_pages(&client, "24.0", &auth.user_id, &auth.user_access_token).await?;

    Ok(Json(fb_pages))
}

#[instrument(skip(client, auth))]
pub async fn page_credentials(
    Path(page_id): Path<String>,
    State(_): State<AppState>,
    Extension(client): Extension<Client>,
    LoggedIn(auth): LoggedIn,
) -> Result<Json<PageCredentials>, Error> {
    let credentials = get_page_credentials(
        &client,
        "24.0",
        &page_id,
        &auth.user_id,
        &auth.user_access_token,
    )
    .await?;
    // .map_err(|err| {
    //     tracing::error!(error = %err, "page credentials failed");
    //     StatusCode::BAD_GATEWAY
    // })?;

    Ok(Json(credentials))
}
