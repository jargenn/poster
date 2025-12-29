use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use r2d2_sqlite::rusqlite::{Connection, params};
use tracing::{debug, instrument};

use crate::cookies::SessionId;
use facebook_graph_api::auth::Authorized;

// TODO: Make this better
pub fn create_database(db_path: impl AsRef<Path>) {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        "BEGIN;
CREATE TABLE if not exists auth_sessions (
    session_id TEXT PRIMARY KEY,
    user_access_token TEXT NOT NULL,
    app_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at INTEGER,         -- unix timestamp (nullable)
    last_verified_at INTEGER    -- unix timestamp
);
        COMMIT;",
    )
    .expect("Failed to create auth_sessions table");
}

#[instrument(
    skip(conn, auth),
    fields(
        session_id = %session_id,
        user_id = %auth.user_id,
        app_id = %auth.app_id
    )
)]
pub fn store_session(conn: &Connection, session_id: &SessionId, auth: &Authorized) -> Result<()> {
    tracing::info!("storing auth session");
    conn.execute(
        r#"
        INSERT INTO auth_sessions (
            session_id,
            user_access_token,
            app_id,
            user_id,
            expires_at,
            last_verified_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            session_id.to_string(),
            auth.user_access_token,
            auth.app_id,
            auth.user_id,
            auth.expires_at.map(|t| t
                .duration_since(UNIX_EPOCH)
                .map(|res| res.as_secs() as i64)
                .unwrap_or(0)),
            auth.last_verified_at
                .duration_since(UNIX_EPOCH)
                .expect("Couldnt get time since UNIX_EPOCH")
                .as_secs() as i64,
        ],
    )?;
    Ok(())
}
#[instrument("Searching for the session_id in the database"
    skip(conn),
    fields(session_id = %session_id.bold())
)]
pub fn load_session(conn: &Connection, session_id: &SessionId) -> Result<Option<Authorized>> {
    debug!("loading session");
    let mut stmt = conn.prepare(
        r#"
        SELECT
            user_access_token,
            app_id,
            user_id,
            expires_at,
            last_verified_at
        FROM auth_sessions
        WHERE session_id = ?1
        "#,
    )?;

    let mut rows = stmt.query(params![session_id.to_string()])?;

    let row = match rows.next()? {
        Some(row) => {
            debug!("session found");
            row
        }
        None => {
            tracing::warn!("session not found");
            return Ok(None);
        }
    };

    let expires_at: Option<i64> = row.get(3)?;
    let last_verified_at: i64 = row.get(4)?;

    Ok(Some(Authorized {
        user_access_token: row.get(0)?,
        app_id: row.get(1)?,
        user_id: row.get(2)?,
        expires_at: expires_at.map(|secs| UNIX_EPOCH + Duration::from_secs(secs as u64)),
        last_verified_at: UNIX_EPOCH + Duration::from_secs(last_verified_at as u64),
    }))
}

#[instrument(skip(conn), fields(session_id = %session_id))]
pub fn delete_session(conn: &Connection, session_id: &SessionId) -> Result<()> {
    let deleted = conn.execute(
        "DELETE FROM auth_sessions WHERE session_id = ?1",
        params![session_id.to_string()],
    )?;
    tracing::info!(deleted, "deleted session");
    Ok(())
}
