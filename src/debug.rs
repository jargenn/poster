use axum::{Extension, Json};
use axum_extra::extract::CookieJar;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use serde::Serialize;
use tracing::instrument;

use crate::{cookies::SessionId, db::load_session};

#[derive(Serialize)]
pub struct DebugSession {
    has_cookie: bool,
    session_id: Option<String>,
    session_found: bool,
    expired: Option<bool>,
    user_id: Option<String>,
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
        }),

        Some(auth) => {
            let expired = auth.expires_at.map(|t| t <= std::time::SystemTime::now());

            Json(DebugSession {
                has_cookie: true,
                session_id: Some(session_id_raw),
                session_found: true,
                expired,
                user_id: Some(auth.user_id),
            })
        }
    }
}
