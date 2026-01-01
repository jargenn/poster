use axum::{
    Extension, Json,
    extract::{Path, State},
};
use color_eyre::owo_colors::OwoColorize;
use http::StatusCode;
use reqwest::Client;
use serde::Deserialize;
use tracing::{error, instrument};

use crate::{db, error::Error, extractors::Auth, server::AppState};
use facebook_graph_api::{
    FacebookPost,
    page_api::{FacebookPages, Page, PostData, get_facebook_pages, get_page_credentials},
};

#[instrument(skip(client, auth))]
pub async fn facebooks_pages(
    State(_): State<AppState>,
    Extension(client): Extension<Client>,
    Auth(auth): Auth,
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
    Auth(auth): Auth,
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
    pub content: String,
    pub scheduled_publish_time: Option<String>,
    pub link: Option<String>,
}

impl TryInto<FacebookPost> for PostPayload {
    type Error = Error;

    fn try_into(self) -> Result<FacebookPost, Self::Error> {
        Ok(FacebookPost::new(
            self.content,
            self.scheduled_publish_time,
            self.link,
        )?)
    }
}

#[instrument(
    "Scheduling a list post in the database for Facebook",
    skip(pool,auth, payload),
    fields(page_id = %page_id.bold())
)]
pub async fn schedule_multiple_posts(
    Path(page_id): Path<String>,
    State(AppState { pool, .. }): State<AppState>,
    Auth(auth): Auth,
    Json(payload): Json<Vec<PostPayload>>,
) -> Result<StatusCode, Error> {
    let mut conn = pool.acquire().await.map_err(Error::Database)?;

    let posts: Vec<FacebookPost> = payload
        .into_iter()
        .map(TryInto::try_into)
        .collect::<Result<_, _>>()?;

    match db::post::schedule_multiple_posts(&mut conn, &auth.user_id, &page_id, posts).await {
        Ok(()) => Ok(StatusCode::ACCEPTED),
        Err(err) => {
            error!(
                "Something bad happened while trying to store the issued post in the database: {err}"
            );
            Err(Error::FailToSchedulePost { page_id })
        }
    }
}

#[instrument(
    "Scheduling a post in the database for Facebook",
    skip(payload, auth, pool),
    fields(page_id = %page_id.bold())
)]
pub async fn schedule_post(
    Path(page_id): Path<String>,
    State(AppState { pool, .. }): State<AppState>,
    Auth(auth): Auth,
    Json(payload): Json<PostPayload>,
) -> Result<StatusCode, Error> {
    let mut conn = pool.acquire().await.map_err(Error::Database)?;

    let post = payload.try_into()?;

    match db::post::schedule_post(&mut conn, &auth.user_id, &page_id, post).await {
        Ok(()) => Ok(StatusCode::ACCEPTED),
        Err(err) => {
            error!(
                "Something bad happened while trying to store the issued post in the database: {err}"
            );
            Err(Error::FailToSchedulePost { page_id })
        }
    }
}

#[axum::debug_handler]
#[instrument(
    "Asking for a list of page post from the Graph API"
    skip(client,auth),
    fields(page_id = %page_id.bold())
)]
pub async fn get_page_posts(
    Path(page_id): Path<String>,
    State(_): State<AppState>,
    Extension(client): Extension<Client>,
    Auth(auth): Auth,
) -> Result<Json<PostData>, Error> {
    let page_credentials = get_page_credentials(
        &client,
        "24.0",
        &page_id,
        &auth.user_id,
        &auth.user_access_token,
    )
    .await?;
    Ok(Json(
        facebook_graph_api::page_api::get_page_posts(
            &client,
            "24.0",
            &page_id,
            &page_credentials.access_token,
        )
        .await?,
    ))
}
