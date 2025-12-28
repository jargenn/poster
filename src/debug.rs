use axum::{Extension, Json};
use axum_extra::extract::CookieJar;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};
use time::OffsetDateTime;
use tracing::instrument;

use crate::{cookies::SessionId, db::load_session};

#[derive(Serialize)]
pub struct DebugSession {
    has_cookie: bool,
    session_id: Option<String>,
    session_found: bool,
    expired: Option<bool>,
    user_id: Option<String>,
    expires_at: Option<String>,       // Human-readable expiration
    expires_in_seconds: Option<i64>,  // Seconds until expiration (negative if expired)
    last_verified_at: Option<String>, // Human-readable last verification
}

#[instrument("Inspecting the cookie jar", skip(pool, cookies))]
pub async fn debug_session(
    Extension(pool): Extension<Pool<SqliteConnectionManager>>,
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

    let session_id_raw = cookie.unwrap().value().to_string();

    let session_id = match SessionId::parse(&session_id_raw) {
        Ok(id) => id,
        Err(_) => {
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
        }
    };

    let conn = pool
        .get()
        .expect("Couldn't get access to a connection in the pool");
    let session = load_session(&conn, &session_id).ok().flatten();

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

            let (expires_at_str, expires_in_seconds) = match auth.expires_at {
                Some(t) => {
                    let expires_at_str = format_system_time(t);
                    let duration = t
                        .duration_since(now)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or_else(|e| -(e.duration().as_secs() as i64));
                    (Some(expires_at_str), Some(duration))
                }
                None => (Some("Never".to_string()), None),
            };

            let last_verified_at_str = format_system_time(auth.last_verified_at);

            Json(DebugSession {
                has_cookie: true,
                session_id: Some(session_id_raw),
                session_found: true,
                expired,
                user_id: Some(auth.user_id),
                expires_at: expires_at_str,
                expires_in_seconds,
                last_verified_at: Some(last_verified_at_str),
            })
        }
    }
}

fn format_system_time(time: SystemTime) -> String {
    let duration = time
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let secs = duration.as_secs() as i64;
    let nanos = duration.subsec_nanos() as i64;

    let datetime = OffsetDateTime::from_unix_timestamp_nanos((secs * 1_000_000_000 + nanos).into())
        .expect("Invalid timestamp");

    // Format as "YYYY-MM-DD HH:MM:SS UTC"
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        datetime.year(),
        datetime.month() as u8,
        datetime.day(),
        datetime.hour(),
        datetime.minute(),
        datetime.second()
    )
}
