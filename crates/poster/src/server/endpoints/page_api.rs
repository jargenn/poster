use axum::{
    Extension, Json,
    extract::{Path, State},
};
use color_eyre::owo_colors::OwoColorize;
use reqwest::Client;
use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{error, instrument};

use crate::{db::store_issued_post, error::Error, extractors::LoggedIn, server::AppState};
use facebook_graph_api::page_api::{FacebookPages, Page, get_facebook_pages, get_page_credentials};

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
) -> Result<Json<Page>, Error> {
    let credentials = get_page_credentials(
        &client,
        "24.0",
        &page_id,
        &auth.user_id,
        &auth.user_access_token,
    )
    .await?;

    Ok(Json(credentials))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostPayload {
    message: String,
}

#[axum::debug_handler]
#[instrument(
    "Scheduling a post in Facebook",
    skip(client, auth, pool),
    fields(page_id = %page_id.bold(), payload = ?payload.bold())
)]
pub async fn post_to_page(
    Path(page_id): Path<String>,
    State(_): State<AppState>,
    Extension(client): Extension<Client>,
    Extension(pool): Extension<SqlitePool>,
    LoggedIn(auth): LoggedIn,
    Json(payload): Json<PostPayload>,
) -> Result<Json<String>, Error> {
    let mut conn = pool
        .acquire()
        .await
        .expect("Couldn't get access to a connection in the pool");

    let credentials = get_page_credentials(
        &client,
        "24.0",
        &page_id,
        &auth.user_id,
        &auth.user_access_token,
    )
    .await?;

    let post_id = facebook_graph_api::page_api::post_to_page(
        &client,
        "24.0",
        &payload.message,
        &page_id,
        &credentials.access_token,
    )
    .await?;

    match store_issued_post(&mut conn, &page_id, &post_id).await {
        Ok(()) => Ok(Json(post_id)),
        Err(err) => {
            error!(
                "Something bad happened while trying to store the issued post in the database: {err}"
            );
            Err(Error::StoreIssuedPost { page_id, post_id })
        }
    }
}
