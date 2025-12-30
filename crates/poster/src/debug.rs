use axum::{Extension, Json};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use sqlx::PgPool;
use std::time::SystemTime;
use time::OffsetDateTime;
use tracing::instrument;

use crate::{cookies::SessionId, db::session::load_session};

#[derive(Serialize)]
pub struct DebugSession {
    has_cookie: bool,
    session_id: Option<String>,
    session_found: bool,
    expired: Option<bool>,
    user_id: Option<String>,
    expires_at: Option<OffsetDateTime>,
    expires_in_seconds: Option<i64>,
    last_verified_at: Option<OffsetDateTime>,
}

#[instrument("Inspecting the cookie jar", skip(pool, cookies))]
pub async fn debug_session(
    Extension(pool): Extension<PgPool>,
    cookies: CookieJar,
) -> Json<DebugSession> {
    let cookie = cookies.get("session_id");

    if cookie.is_none() {
        return Json(DebugSession {
            has_cookie: false,
            session_id: None,
            session_found: false,
            expired: None,
            user_id: None,
            expires_at: None,
            expires_in_seconds: None,
            last_verified_at: None,
        });
    }

    let session_id_raw = cookie
        .expect("Couldn't extract the value from the cookie")
        .value()
        .to_string();

    let Ok(session_id) = SessionId::parse(&session_id_raw) else {
        return Json(DebugSession {
            has_cookie: true,
            session_id: Some(session_id_raw),
            session_found: false,
            expired: None,
            user_id: None,
            expires_at: None,
            expires_in_seconds: None,
            last_verified_at: None,
        });
    };

    let mut conn = pool
        .acquire()
        .await
        .expect("Couldn't get access to a connection in the pool");
    let session = load_session(&mut conn, &session_id).await.ok().flatten();

    match session {
        None => Json(DebugSession {
            has_cookie: true,
            session_id: Some(session_id_raw),
            session_found: false,
            expired: None,
            user_id: None,
            expires_at: None,
            expires_in_seconds: None,
            last_verified_at: None,
        }),

        Some(auth) => {
            let now = SystemTime::now();
            let expired = auth.expires_at.map(|t| t <= now);

            let (expires_at, expires_in_seconds) = match auth.expires_at {
                Some(t) => {
                    let duration = t.duration_since(now).map_or_else(
                        |e| -e.duration().as_secs().cast_signed(),
                        |d| d.as_secs().cast_signed(),
                    );
                    (Some(t.into()), Some(duration))
                }
                None => (None, None),
            };

            Json(DebugSession {
                has_cookie: true,
                session_id: Some(session_id_raw),
                session_found: true,
                expired,
                user_id: Some(auth.user_id),
                expires_at,
                expires_in_seconds,
                last_verified_at: Some(auth.last_verified_at.into()),
            })
        }
    }
}
