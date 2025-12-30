use std::{
    fmt::Display,
    time::{Duration, UNIX_EPOCH},
};

use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use sqlx::SqliteConnection;
use tracing::{debug, error, info, instrument};

use crate::cookies::SessionId;
use facebook_graph_api::auth::Authorized;

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

    let session_id = session_id.to_string();
    let expires_at = auth.expires_at.map(|t| {
        t.duration_since(UNIX_EPOCH)
            .map(|res| res.as_secs() as i64)
            .unwrap_or(0)
    });
    let last_verified_at = auth
        .last_verified_at
        .duration_since(UNIX_EPOCH)
        .expect("Couldnt get time since UNIX_EPOCH")
        .as_secs() as i64;

    let inserted = sqlx::query!(
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
        session_id,
        auth.user_access_token,
        auth.app_id,
        auth.user_id,
        expires_at,
        last_verified_at,
    )
    .execute(conn)
    .await
    .expect("Failed to insert new sessions in the database")
    .rows_affected();

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
pub async fn store_issued_post(
    conn: &mut SqliteConnection,
    page_id: &str,
    post_id: &str,
) -> Result<()> {
    tracing::info!("storing issued post");

    let status = PostStatus::Pending.to_string();
    let inserted = sqlx::query!(
        "
            INSERT INTO posts_issued (
                post_id,
                page_id,
                status,
                checked_at
            )
            VALUES (?1, ?2, ?3, ?4)
        ",
        post_id,
        page_id,
        status,
        "not yet",
    )
    .execute(conn)
    .await
    .expect("Failed to insert new issued posts into the database")
    .rows_affected();
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

    let session_id = session_id.to_string();
    let row = match sqlx::query_as!(
        SessionRow,
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
        session_id
    )
    .fetch_optional(conn)
    .await
    {
        Ok(opt) => match opt {
            None => {
                debug!(%session_id,"no session found for session_id");
                return Ok(None);
            }
            Some(r) => {
                debug!("session found");
                r
            }
        },
        Err(e) => {
            error!(%e,"Error while search for session in auth_sessions table");
            return Err(eyre::eyre!(
                "Error while search for session in auth_sessions table: {e}"
            ));
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
