use axum_extra::extract::cookie::{Cookie, SameSite};

#[derive(Debug, Clone)]
pub struct SessionId(String);

#[derive(Debug)]
pub struct InvalidSessionID;

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    pub fn parse(s: &str) -> Result<Self, InvalidSessionID> {
        let uuid = uuid::Uuid::parse_str(s).map_err(|_| InvalidSessionID)?;

        Ok(Self(uuid.to_string()))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn build_session_cookie(session_id: SessionId) -> Cookie<'static> {
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
