use axum_extra::extract::cookie::{Cookie, SameSite};
use uuid::Uuid;

/// Represents the `session_id` stored in the cookie jar of the client and in the server's database
/// to be able to search the corresponding `session_data` stored in the database.
///
/// It is intended to be a unique identifier.
#[derive(Debug, Clone)]
pub struct SessionId(Uuid);

/// Possible error value when trying to parse a string to UUID.
#[derive(Debug)]
pub struct InvalidSessionID;

impl SessionId {
    /// Creates a new `SessionID` with a UUID v4
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Makes sure the input string of hexadecimal digits with optional-hyphens is a valid uuid.
    pub fn parse(s: &str) -> Result<Self, InvalidSessionID> {
        let uuid = Uuid::try_parse(s).map_err(|_| InvalidSessionID)?;

        Ok(Self(uuid))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

pub fn build_session_cookie(session_id: &SessionId) -> Cookie<'static> {
    tracing::debug!(
        session_id = %session_id,
        "building session cookie"
    );

    Cookie::build(("session_id", session_id.to_string()))
        .http_only(true)
        .same_site(SameSite::Lax)
        .path("/")
        .secure(false) // true in prod
        .build()
}

pub fn clear_session_cookie() -> Cookie<'static> {
    tracing::debug!("clearing session cookie");
    Cookie::build(("session_id", ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build()
}
