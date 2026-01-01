use axum::extract::{Json, State};
use http::StatusCode;
use serde_json::{Value, json};
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::{UserConfig, error::Error, server::AppState};

/// Saves the user config in the database and eagerly loads it into the cache.
#[instrument(
    "Storing user config to the database",
    skip(state, payload),
    fields(app_id)
)]
pub async fn save(
    State(state): State<AppState>,
    Json(payload): Json<UserConfig>,
) -> Result<(StatusCode, Json<Value>), Error> {
    let mut conn = state.pool.acquire().await.map_err(Error::Database)?;
    let cache = state.cache;

    let id = Uuid::new_v4();
    let config_data = &payload.config_data;
    let app_id = &config_data.app_id;

    sqlx::query!(
        "INSERT INTO user_configs(id, app_id, app_secret,app_config_id, redirect_url, description) VALUES ($1,$2,$3,$4,$5,$6);",
        id,
        app_id,
        config_data.app_secret,
        config_data.app_config_id,
        config_data.redirect_url.to_string(),
        payload.description
    ).execute(&mut *conn).await.map_err(Error::Database)?;

    debug!("config stored in database");

    cache.insert(id.to_string(), payload.config_data).await;
    debug!("config stored in cache");

    let res = json!({"message:": "Config saved!", "config_key": id.to_string()});

    Ok((StatusCode::ACCEPTED, Json(res)))
}
