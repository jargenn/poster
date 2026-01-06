use axum::{
    Json,
    response::{IntoResponse, Response},
};
use facebook_graph_api::ScheduledTimeError;
use http::{HeaderValue, StatusCode, header::WWW_AUTHENTICATE};
use serde_json::json;
use tower_sessions::session;

use crate::{authentication, media::MediaError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Facebook(#[from] facebook_graph_api::Error),
    #[error(transparent)]
    SchedulingError(#[from] PostSchedulingError),
    #[error("Network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Authentication failed.")]
    AuthError(#[from] authentication::AuthError),
    #[error("Session error. You are probably not logged in")]
    SessionError(#[from] session::Error),
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::Facebook(err) => err.into_response(),
            Error::Reqwest(err) => {
                tracing::error!(error = %err, "upstream service error");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": "Upstream service error" })),
                )
                    .into_response()
            }
            Error::Json(err) => {
                tracing::warn!(error = %err, "invalid json payload");
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": "Invalid JSON payload" })),
                )
                    .into_response()
            }
            Error::Database(error) => {
                tracing::error!(error= %error, "internal server error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response()
            }
            Error::SchedulingError(error) => {
                tracing::error!(error = %error, "internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response()
            }
            Error::AuthError(report) => {
                tracing::error!(error = %report, "auth error");

                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#)
                    .expect("Failed to create header value");
                let mut response = (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": report.to_string() })),
                )
                    .into_response();

                response
                    .headers_mut()
                    .insert(WWW_AUTHENTICATE, header_value);

                response
            }
            Error::SessionError(error) => {
                tracing::error!(error = %error, "internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error.to_string() })),
                )
                    .into_response()
            }
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PostSchedulingError {
    #[error(transparent)]
    MediaPostError(#[from] MediaError),
    #[error(transparent)]
    DateError(#[from] ScheduledTimeError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
}
