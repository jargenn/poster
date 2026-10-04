use axum::extract::{Json, State};
use eyre::OptionExt;
use http::StatusCode;
use serde_json::{Value, json};
use tracing::{debug, instrument};

use crate::{
    authentication::AuthError, configuration::UserConfig, error::Error, server::PosterState,
    session_state::TypedSession,
};

/// Saves the user config in the database and eagerly loads it into the cache.
#[instrument(
    "Storing user config to the database",
    skip(state, payload, session),
    fields(app_id)
)]
pub async fn save(
    State(state): State<PosterState>,
    session: TypedSession,
    Json(payload): Json<UserConfig>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let mut conn = state.pool.acquire().await.map_err(Error::Database)?;

    let user_id = session
        .get_user_id()
        .await
        .map_err(Error::SessionError)?
        .ok_or_eyre("`user_id` session is empty")
        .map_err(AuthError::UnexpectedError)?;

    let config_data = &payload.config_data;
    let app_id = &config_data.app_id;
    let user_id = user_id.to_string();

    sqlx::query(
        "INSERT INTO user_configs(id, app_id, app_secret, app_config_id, redirect_url, description) VALUES ($1,$2,$3,$4,$5,$6);",
    )
    .bind(&user_id)
    .bind(app_id)
    .bind(&config_data.app_secret)
    .bind(&config_data.app_config_id)
    .bind(config_data.redirect_url.to_string())
    .bind(payload.description)
    .execute(&mut *conn).await.map_err(Error::Database)?;

    debug!("config stored in database");

    state
        .fb_app_config
        .insert(user_id, payload.config_data)
        .await;

    debug!("config stored in cache");

    let res = json!({"message:": "Config saved!"});

    Ok((StatusCode::ACCEPTED, Json(res)))
}
