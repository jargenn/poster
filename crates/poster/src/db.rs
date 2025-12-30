use std::{
    fmt::Display,
    time::{Duration, UNIX_EPOCH},
};

use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use sqlx::{Connection, SqliteConnection};
use tokio::runtime::Handle;
use tracing::{debug, error, info, instrument};

use crate::cookies::SessionId;
use facebook_graph_api::auth::Authorized;

// TODO: Make this better
pub fn create_database(db_path: &str) {
    Handle::current().block_on(async { 
        let mut conn = SqliteConnection::connect(db_path).await.unwrap();
        sqlx::query(
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
            ).execute(&mut conn).await.expect("Failed to create auth_sessions table");

        sqlx::query(
        "BEGIN;
            CREATE TABLE if not exists posts_issued (
                post_id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                checked_at TEXT,
                status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending', 'published', 'failed')),
                page_id TEXT NOT NULL,
                check_attempts INTEGER NOT NULL DEFAULT 0
            );
        COMMIT;",
            ).execute(&mut conn).await.expect("Failed to create posts_issued table");
    });
}

#[instrument("Storing session data",
    skip(conn, auth),
    fields(
        session_id = %session_id,
        user_id = %auth.user_id,
        app_id = %auth.app_id
    )
)]
pub async fn store_session(
    conn: &mut SqliteConnection,
    session_id: &SessionId,
    auth: &Authorized,
) -> Result<()> {
    tracing::info!("storing auth session");

        let inserted =sqlx::query(
            "INSERT INTO auth_sessions (
                session_id,
                user_access_token,
                app_id,
                user_id,
                expires_at,
                last_verified_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ",
        )
        .bind(session_id.to_string())
        .bind(&auth.user_access_token)
        .bind(&auth.app_id)
        .bind(&auth.user_id)
        .bind(auth.expires_at.map(|t| {
            t.duration_since(UNIX_EPOCH)
                .map(|res| res.as_secs() as i64)
                .unwrap_or(0)
        }))
        .bind(
            auth.last_verified_at
                .duration_since(UNIX_EPOCH)
                .expect("Couldnt get time since UNIX_EPOCH")
                .as_secs() as i64,
        )
        .execute(conn)
        .await.expect("Failed to insert new sessions in the database").rows_affected()
    ;

    info!("{inserted} session_data in the database");
    Ok(())
}

#[derive(Debug)]
pub enum PostStatus {
    Failed,
    Pending,
    Published,
}

impl Display for PostStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let out = match self {
            PostStatus::Failed => "failed",
            PostStatus::Pending => "pending",
            PostStatus::Published => "published",
        };
        write!(f, "{out}")
    }
}

#[instrument("Storing issued post", skip(conn))]
pub async fn store_issued_post(conn: &mut SqliteConnection, page_id: &str, post_id: &str) -> Result<()> {
    tracing::info!("storing issued post");

    // let inserted = Handle::current().block_on(async {
        let inserted = sqlx::query(
            "
            INSERT INTO posts_issued (
                post_id,
                page_id,
                status,
                checked_at
            )
            VALUES (?1, ?2, ?3, ?4)
        ",
        )
        .bind(post_id.to_string())
        .bind(page_id.to_string())
        .bind(PostStatus::Pending.to_string())
        .bind("not yet".to_string())
        .execute(conn)
        .await.expect("Failed to insert new issued posts into the database").rows_affected()
    ;
    info!("{inserted} issued post stored in the database");
    Ok(())
}

#[instrument("Searching for the session_id in the database"
    skip(conn),
    fields(session_id = %session_id.bold())
)]
pub async fn load_session(
    conn: &mut SqliteConnection,
    session_id: &SessionId,
) -> Result<Option<Authorized>> {
    debug!("searching for session_id in the database");

    #[derive(Debug, sqlx::FromRow)]
    struct SessionRow {
        user_access_token: String,
        app_id: String,
        user_id: String,
        expires_at: Option<i64>,
        last_verified_at: i64,
    }

        let row: SessionRow = match sqlx::query_as(
            "
            SELECT
                user_access_token,
                app_id,
                user_id,
                expires_at,
                last_verified_at
            FROM auth_sessions
            WHERE session_id = ?1
            ",
        )
        .bind(session_id.to_string())
        .fetch_optional(conn)
        .await {
            Ok(opt) => match opt {
                None => {
                    debug!(%session_id,"no session found for session_id");
                    return Ok(None)
                },
                Some(r) => {
                    debug!("session found");
                    r
                }
            },
            Err(e) => {
                error!(%e,"Error while search for session in auth_sessions table");
                return Err(eyre::eyre!("Error while search for session in auth_sessions table: {e}"))
            }
        };

    Ok(Some(Authorized {
        user_access_token: row.user_access_token,
        app_id: row.app_id,
        user_id: row.user_id,
        expires_at: row
            .expires_at
            .map(|secs| UNIX_EPOCH + Duration::from_secs(secs as u64)),
        last_verified_at: UNIX_EPOCH + Duration::from_secs(row.last_verified_at as u64),
    }))

}

// #[instrument(skip(conn), fields(session_id = %session_id))]
// pub fn delete_session(conn: &mut SqliteConnection, session_id: &SessionId) -> Result<()> {
//     let deleted = Handle::current().block_on(async {
//         sqlx::query("DELETE FROM auth_sessions WHERE session_id = ?1").bind(session_id.to_string()).execute(conn).await.expect("Failed to execute delete statement").rows_affected()
//     });
//     tracing::info!(deleted, "deleted session");
//     Ok(())
// }
