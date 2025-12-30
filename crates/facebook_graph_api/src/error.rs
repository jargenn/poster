use std::num::{NonZeroU16, NonZeroU32};

use serde::Deserialize;

#[cfg(feature = "axum")]
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
#[cfg(feature = "axum")]
use serde_json::json;

use crate::ErrorCode;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // TODO: Wouldn't this be repeated from what ErrorCode could tell me?
    #[error(transparent)]
    Auth(#[from] AuthError),
    #[error(transparent)]
    GraphApi(#[from] GraphApiError),
    #[error("Network error: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Business error: {0}")]
    Business(#[from] BusinessError),
    #[error("App error: {0}")]
    App(#[from] AppError),
}

#[cfg(feature = "axum")]
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Error::Auth(err) => {
                tracing::warn!(error = %err, "authentication failed");
                (StatusCode::UNAUTHORIZED, err.to_string())
            }
            Error::GraphApi(err) => {
                tracing::error!(error = %err, code = %err.code, "facebook graph api error");
                (err.code.into(), err.to_string())
            }
            Error::Reqwest(err) => {
                tracing::error!(error = %err, "upstream service error");
                (StatusCode::BAD_GATEWAY, "Upstream service error".into())
            }
            Error::Json(err) => {
                tracing::warn!(error = %err, "invalid json payload");
                (StatusCode::BAD_REQUEST, "Invalid JSON payload".into())
            }
            Error::Business(err) => {
                tracing::warn!(error = %err, "business logic error");
                (StatusCode::UNPROCESSABLE_ENTITY, err.to_string())
            }
            Error::App(err) => {
                tracing::error!(error = %err, "internal application error");
                (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
            }
        };

        let body = Json(json!({
            "error": message,
        }));
        (status, body).into_response()
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error("{code}:{error_type} (trace id: {trace_id})")]
pub struct GraphApiError {
    pub error_type: String,
    pub code: ErrorCode,
    pub user_title: Option<String>,
    pub user_message: Option<String>,
    pub trace_id: String,
}

impl GraphApiError {
    /// Parse a Graph API error from JSON response body
    pub fn from_response_body(body: &str) -> Result<Self, serde_json::Error> {
        let response: GraphApiErrorResponse = serde_json::from_str(body)?;
        let data = response.error;

        let code = ErrorCode {
            code: NonZeroU32::new(data.code).unwrap_or(NonZeroU32::new(1).unwrap()),
            subcode: data.error_subcode.and_then(NonZeroU16::new),
        };

        // Assert that Facebook's message matches our canonical representation
        let canonical = code.canonical_reason();
        if canonical != Some("Unknown error")
            && !data
                .message
                .contains(code.canonical_reason().unwrap_or_default())
        {
            tracing::warn!(
                code = %code,
                expected = canonical,
                actual = %data.message,
                "Facebook error message differs from canonical representation - API may have changed"
            );
        }

        Ok(GraphApiError {
            error_type: data.error_type,
            code,
            user_title: data.error_user_title,
            user_message: data.error_user_msg,
            trace_id: data.trace_id,
        })
    }
}

#[derive(Debug, Deserialize)]
struct GraphApiErrorResponse {
    error: GraphApiErrorData,
}

#[derive(Debug, Deserialize)]
struct GraphApiErrorData {
    message: String,
    #[serde(rename = "type")]
    error_type: String,
    code: u32,
    error_subcode: Option<u16>,
    error_user_title: Option<String>,
    error_user_msg: Option<String>,
    #[serde(rename = "fb_trace_id")]
    trace_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("The access token you are using doesn't belong to the this App")]
    WrongApp,
    #[error("Provided access token is no longer valid")]
    InvalidToken,
    #[error("You are not logged in to the App")]
    NotLoggedIn,
    #[error("Access token has expired")]
    Expired,
}

#[derive(Debug, thiserror::Error)]
pub enum BusinessError {
    #[error("The page ({page_id}) was not found in the pages the user ({user_id}) has access to")]
    PageNotFound { page_id: String, user_id: String },
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(
        "An error ocurred while trying to store issued post ({post_id}) for the page ({page_id} in the database."
    )]
    StoreIssuedPost { page_id: String, post_id: String },
}
