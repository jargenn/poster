use axum::{Json, extract::State};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use tracing::instrument;

use crate::{AppState, cookies::SessionId, db::load_session};

#[derive(Serialize)]
pub struct DebugSession {
    has_cookie: bool,
    session_id: Option<String>,
    session_found: bool,
    expired: Option<bool>,
    user_id: Option<String>,
}

#[instrument(skip(app, cookies))]
pub async fn debug_session(State(app): State<AppState>, cookies: CookieJar) -> Json<DebugSession> {
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

    let session = load_session(&app.db, &session_id).ok().flatten();

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
