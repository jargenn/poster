use std::num::NonZeroU32;

use serde::Deserialize;

#[cfg(feature = "axum")]
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
#[cfg(feature = "axum")]
use serde_json::json;

use crate::{ErrorCode, PostError};

#[derive(Debug, thiserror::Error)]
#[error("Facebook client code")]
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
    #[error("The page ({page_id}) was not found in the pages the user ({user_id}) has access to")]
    PageNotFound { page_id: String, user_id: String },
    #[error(transparent)]
    PostError(#[from] PostError),
}

#[cfg(feature = "axum")]
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        match self {
            Error::PostError(post_error) => {
                // now post_error: PostError (owned)
                post_error.into_response()
            }

            Error::Auth(err) => {
                tracing::warn!(error = %err, "authentication failed");
                (
                    StatusCode::UNAUTHORIZED,
                    Json(json!({ "error": err.to_string() })),
                )
                    .into_response()
            }

            Error::GraphApi(err) => {
                tracing::error!(error = %err, code = %err.code, "facebook graph api error");
                (
                    StatusCode::from(err.code),
                    Json(json!({ "error": err.to_string() })),
                )
                    .into_response()
            }

            Error::Reqwest(err) => {
                tracing::error!(error = %err, "upstream service error");
                (
                    StatusCode::BAD_GATEWAY,
                    Json(json!({ "error": "Upstream service error" })),
                )
                    .into_response()
            }

            Error::PageNotFound { .. } => {
                tracing::warn!(error = %self);
                (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({ "error": self.to_string() })),
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
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
#[error(
    "Graph API Error\n  Code: {code}\n  Type: {error_type}\n  Reason: {}\n  Details: {help_message}\n  Trace: {trace_id}", code.canonical_reason().unwrap_or("Unknown reason")
)]
pub struct GraphApiError {
    pub help_message: String,
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
            code: NonZeroU32::new(data.code)
                .unwrap_or(NonZeroU32::new(1).expect("Couldn't get a NonZeroU32 from 1")),
            subcode: data.error_subcode.and_then(NonZeroU32::new),
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
            help_message: data.message,
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
    error_subcode: Option<u32>,
    error_user_title: Option<String>,
    error_user_msg: Option<String>,
    #[serde(rename = "fbtrace_id")]
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

#[cfg(test)]
mod tests {
    // As to 28/12/2025, Facebook has disabled test users so they give me no choice but to Mock their
    // API by Graph API explorer to see their error messages

    use super::*;
    use expect_test::{Expect, expect};
    use reqwest::Client;
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path, query_param},
    };

    fn check(status: ErrorCode, expect: Expect) {
        expect.assert_debug_eq(&status);
    }

    fn check_reason(canonical_reason: Option<&str>, expect: Expect) {
        expect.assert_debug_eq(&canonical_reason);
    }

    #[tokio::test]
    async fn facebook_v24_0_me_190() {
        let mock_server = MockServer::start().await;

        let error_body = json!({
            "error": {
                "message": "Error validating access token: Session has expired.",
                "type": "OAuthException",
                "code": 190,
                "error_subcode": 463,
                "fbtrace_id": "Ab7Udf3hEvk7tW7RrcY8No9"
            }
        });

        Mock::given(method("GET"))
            .and(path("/me"))
            .and(query_param("fields", "id,name"))
            .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
            .expect(1)
            .mount(&mock_server)
            .await;

        let client = Client::new();

        let resp = client
            .get(format!("{}/me", mock_server.uri()))
            .query(&[("fields", "id,name")])
            .send()
            .await
            .expect("request failed");

        assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

        let body: serde_json::Value = resp.json().await.expect("invalid json");

        let err = &body["error"];

        let code = err["code"].as_u64().unwrap() as u32;
        let subcode = err["error_subcode"].as_u64().unwrap() as u32;

        let status = ErrorCode::from_parts(code, Some(subcode)).expect("invalid fb status code");

        assert_eq!(status, ErrorCode::TOKEN_EXPIRED);
        check(
            status,
            expect![[r#"
            ErrorCode(190, Some(463))
        "#]],
        );
        check_reason(
            status.canonical_reason(),
            expect![[r#"
            Some(
                "Login status or access token has expired, been revoked, or is invalid",
            )
        "#]],
        );
    }
}
