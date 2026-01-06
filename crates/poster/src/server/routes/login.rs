use axum::{
    Form, Json,
    extract::State,
    response::{IntoResponse, Response},
};
use http::StatusCode;
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::json;
use tracing::instrument;

use crate::{
    authentication::{AuthError, Credentials, validate_credentials},
    server::PosterState,
    session_state::TypedSession,
};

#[derive(Deserialize, Debug)]
pub struct FormData {
    pub username: String,
    pub password: SecretString,
}

#[instrument("Login", skip(state, payload, session))]
pub async fn login(
    State(state): State<PosterState>,
    session: TypedSession,
    Form(payload): Form<FormData>,
) -> Result<Response, LoginError> {
    let credentials = Credentials {
        username: payload.username,
        password: payload.password,
    };
    tracing::Span::current().record("username", &tracing::field::display(&credentials.username));

    let user_id = validate_credentials(credentials, &state.pool)
        .await
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
            AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
        })?;

    tracing::Span::current().record("user_id", &tracing::field::display(&user_id));

    session
        .insert_user_id(user_id)
        .await
        .map_err(|e| LoginError::UnexpectedError(eyre::eyre!("{e}")))?;

    Ok(StatusCode::OK.into_response())
}

#[derive(thiserror::Error, Debug)]
pub enum LoginError {
    #[error("Authentication failed")]
    AuthError(#[source] eyre::Error),
    #[error("Something went wrong")]
    UnexpectedError(#[from] eyre::Error),
}

impl IntoResponse for LoginError {
    fn into_response(self) -> Response {
        let (status_code, help_message) = match self {
            LoginError::AuthError(_) => (
                StatusCode::UNAUTHORIZED,
                "The user credentials given are invalid. Please check if you wrote them correctly.",
            ),
            LoginError::UnexpectedError(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "An unexpected error happened. Please try again.",
            ),
        };

        (status_code, Json(json!({ "error": help_message }))).into_response()
    }
}
