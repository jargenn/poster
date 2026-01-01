use axum::{
    Json,
    response::{IntoResponse, Response},
};
use http::StatusCode;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Facebook(#[from] facebook_graph_api::Error),
    #[error(
        "An error ocurred while trying to schedule a post for the page ({page_id}) in the database."
    )]
    FailToSchedulePost { page_id: String },
    #[error("Network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
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
            Error::FailToSchedulePost { .. } => {
                tracing::error!(error = %self, "internal application error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": self.to_string() })),
                )
                    .into_response()
            }
        }
    }
}
