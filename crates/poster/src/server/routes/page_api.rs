use axum::{
    Extension, Json,
    extract::{Path, State},
};
use axum_extra::TypedHeader;
use base64::{Engine as _, engine::general_purpose};
use color_eyre::owo_colors::OwoColorize;
use eyre::{Context, ContextCompat};
use http::{HeaderMap, StatusCode};
use reqwest::Client;
use secrecy::SecretString;
use serde::Deserialize;
use sqlx::SqliteConnection;
use tracing::instrument;

use crate::{
    IdempotencyKey,
    authentication::{self, Credentials},
    error::Error,
    extractors::Auth,
    server::PosterState,
    session_state::TypedSession,
    storage::db::{self},
};
use facebook_graph_api::{
    FacebookPost, Input,
    page_api::{FacebookPages, Page, get_facebook_pages, get_page_credentials},
};
use auth::Authorized;

#[instrument(skip(client, auth))]
pub async fn facebooks_pages(
    State(state): State<PosterState>,
    Extension(client): Extension<Client>,
    Auth(auth): Auth,
) -> Result<Json<FacebookPages>, Error> {
    let fb_pages = get_facebook_pages(
        &client,
        "24.0",
        &auth.user_id,
        &auth.user_access_token,
        state.facebook_uri,
    )
    .await?;

    Ok(Json(fb_pages))
}

async fn get_auth(session: TypedSession, conn: &mut SqliteConnection) -> Result<Authorized, Error> {
    let Some(user_id) = session.get_user_id().await.map_err(Error::SessionError)? else {
        todo!("None user_id")
    };

    let Some(auth) = db::session::load_session(conn, &user_id)
        .await
        .map_err(|e| authentication::AuthError::UnexpectedError(e))?
    else {
        todo!("None auth")
    };

    Ok(auth)
}

#[instrument(skip(client, session))]
pub async fn page_credentials(
    Path(page_id): Path<String>,
    State(state): State<PosterState>,
    Extension(client): Extension<Client>,
    session: TypedSession,
) -> Result<Json<Page>, Error> {
    let mut conn = state.pool.acquire().await.map_err(Error::Database)?;

    let auth = get_auth(session, &mut conn).await?;

    let credentials = get_page_credentials(
        &client,
        "24.0",
        &page_id,
        &auth.user_id,
        &auth.user_access_token,
        state.facebook_uri,
    )
    .await?;

    Ok(Json(credentials))
}

#[derive(Debug, Deserialize)]
pub(crate) struct PostPayload {
    pub content: String,
    pub scheduled_publish_time: Option<String>,
    pub link: Option<String>,
    #[serde(alias = "media_url")]
    pub media: Option<Vec<Input>>,
}

impl TryInto<FacebookPost> for PostPayload {
    type Error = Error;

    fn try_into(self) -> Result<FacebookPost, Self::Error> {
        Ok(FacebookPost::new(
            self.content,
            self.scheduled_publish_time,
            self.link,
            self.media,
        )?)
    }
}

fn basic_auth(headers: &HeaderMap) -> eyre::Result<Credentials> {
    let header_value = headers
        .get("Authorization")
        .context("The 'Authorization' header was missing")?
        .to_str()
        .context("The 'Authorization' header was not a valid UTF8 string.")?;

    let base64encoded_segment = header_value
        .strip_prefix("Basic ")
        .context("The authorization scheme was not 'Basic'.")?;

    let decoded_bytes = general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to base64-decode 'Basic' credentials.")?;

    let decoded_credentials = String::from_utf8(decoded_bytes)
        .context("The decoded credential string is not valid UTF8.")?;

    // Split into two segments, using ':' as delimitator
    let mut credentials = decoded_credentials.splitn(2, ':');
    let username = credentials
        .next()
        .ok_or_else(|| eyre::eyre!("A username must be provided in 'Basic' auth."))?
        .to_string();
    let password = credentials
        .next()
        .ok_or_else(|| eyre::eyre!("A password must be provided in 'Basic' auth."))?
        .to_string();

    Ok(Credentials {
        username,
        password: SecretString::new(password.into()),
    })
}

#[axum::debug_handler]
#[instrument(
    "Scheduling posts",
    skip(state,session, payload, ),
    // skip(state,session, payload, idempotency_key),
    fields(page_id = %page_id.bold())
)]
pub async fn schedule_posts(
    // TypedHeader(idempotency_key): TypedHeader<IdempotencyKey>,
    session: TypedSession,
    Path(page_id): Path<String>,
    State(state): State<PosterState>,
    Json(payload): Json<Vec<PostPayload>>,
) -> Result<StatusCode, Error> {
    let posts: Vec<FacebookPost> = payload
        .into_iter()
        .map(TryInto::try_into)
        .collect::<Result<_, _>>()?;

    let pool = state.pool;
    let mut conn = pool.acquire().await.map_err(Error::Database)?;
    let auth = get_auth(session, &mut conn).await?;

    let media_settings = state.media_settings;
    let facebook_uri = state.facebook_uri;

    db::post::submit_posts(
        &mut conn,
        &auth.user_id,
        &page_id,
        &auth.user_access_token,
        posts,
        media_settings,
        facebook_uri,
    )
    .await
    .map_err(Error::from)?;

    Ok(StatusCode::ACCEPTED)
}

// // TODO: Fails
// #[instrument(
//     "Asking for a list of page post from the Graph API"
//     skip(client,auth),
//     fields(page_id = %page_id.bold())
// )]
// pub async fn get_page_posts(
//     Path(page_id): Path<String>,
//     State(_): State<PosterState>,
//     Extension(client): Extension<Client>,
//     Auth(auth): Auth,
// ) -> Result<Json<PostData>, Error> {
//     let page_credentials = get_page_credentials(
//         &client,
//         "24.0",
//         &page_id,
//         &auth.user_id,
//         &auth.user_access_token,
//     )
//     .await?;
//     Ok(Json(
//         facebook_graph_api::page_api::get_page_posts(
//             &client,
//             "24.0",
//             &page_id,
//             &page_credentials.access_token,
//         )
//         .await?,
//     ))
// }
