use axum::{
    extract::{FromRef, FromRequestParts},
    http::{StatusCode, request::Parts},
};
use axum_extra::extract::CookieJar;
use tracing::instrument;

use crate::{cookies::SessionId, db::session::load_session, server::AppState};
use facebook_graph_api::auth::Authorized;

pub struct Auth(pub Authorized);

impl<S> FromRequestParts<S> for Auth
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = StatusCode;

    #[instrument(skip(parts, state))]
    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        tracing::debug!("attempting LoggedIn extraction");

        let cookies = CookieJar::from_request_parts(parts, state)
            .await
            .map_err(|_| {
                tracing::error!("cookie jar coulnd't be read");
                StatusCode::UNAUTHORIZED
            })?;

        let cookie = cookies.get("session_id").ok_or({
            tracing::error!("`session_id` is not in the cookie jar");
            StatusCode::UNAUTHORIZED
        })?;

        let session_id = SessionId::parse(cookie.value()).map_err(|_| {
            tracing::error!("The session_id stored in the client is not a valid UUID v4");
            StatusCode::BAD_REQUEST
        })?;

        tracing::debug!(session_id = %session_id, "session cookie found");

        let app_state = AppState::from_ref(state);
        let mut conn = app_state.pool.acquire().await.map_err(|err| {
            tracing::error!(
                error = err.to_string(),
                "Failed to acquire handle to database connection in Auth middleware"
            );
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        let auth = load_session(&mut conn, &session_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        if auth.locally_expired() {
            tracing::warn!("session expired locally");
            return Err(StatusCode::UNAUTHORIZED);
        }

        if auth.needs_remote_verification() {
            tracing::error!("session needs remote verification");
            return Err(StatusCode::UNAUTHORIZED);
        }

        Ok(Auth(auth))
    }
}
