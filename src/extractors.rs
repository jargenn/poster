use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::CookieJar;
use tracing::instrument;

use crate::{AppState, cookies::SessionId, db::load_session, login::Authorized};

pub struct LoggedIn(pub Authorized);

impl<S> FromRequestParts<S> for LoggedIn
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = StatusCode;

    #[instrument(skip(parts, state))]
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        tracing::debug!("attempting LoggedIn extraction");

        let app = AppState::from_ref(state);
        let cookies = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;

        let cookie = cookies.get("session_id").ok_or(StatusCode::UNAUTHORIZED)?;
        let session_id = SessionId::parse(cookie.value()).map_err(|_| StatusCode::UNAUTHORIZED)?;

        tracing::debug!(session_id = %session_id, "session cookie found");

        let auth = load_session(&app.db, &session_id)
            .map_err(|_| {
                tracing::warn!("session lookup failed");
                StatusCode::INTERNAL_SERVER_ERROR
            })?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        if auth.locally_expired() {
            tracing::warn!("session expired locally");
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(LoggedIn(auth))
    }
}
