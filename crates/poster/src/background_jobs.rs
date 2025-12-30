// TODO: Take in consideration that fetch_all loads everything it finds to memory, so in production
// I should limit the number of elements it returns.
use std::time::{Duration, UNIX_EPOCH};

use reqwest::Client;
use sqlx::{Connection, SqliteConnection};
use tracing::{info, instrument};

use facebook_graph_api::auth::Authorized;

#[instrument("cleaning and verifying sessions", skip(db_path, app_secret))]
pub async fn maintain_sessions(db_path: &str, app_id: &str, app_secret: &str) {
    let app_id = app_id.to_string();
    let app_secret = app_secret.to_string();
    let client = Client::new();

    let mut conn = SqliteConnection::connect(db_path)
        .await
        .expect("Failed to open a connection to the sqlite db");
    let deleted = sqlx::query!("DELETE FROM auth_sessions WHERE expires_at < unixepoch('now')")
        .execute(&mut conn)
        .await
        .expect("Delete session query failed")
        .rows_affected();
    if deleted > 0 {
        info!(deleted, "expired sessions cleaned up");
    }

    #[derive(Debug)]
    struct AuthRows {
        session_id: String,
        user_access_token: String,
        app_id: String,
        user_id: String,
        expires_at: Option<i64>,
        last_verified_at: i64,
    }

    let sessions: Vec<(String, Authorized)> = {
        let rows = sqlx::query_as!(
            AuthRows,
            r#"
                SELECT
                    session_id        AS "session_id!",
                    user_access_token AS "user_access_token!",
                    app_id            AS "app_id!",
                    user_id           AS "user_id!",
                    expires_at,
                    last_verified_at AS "last_verified_at!"
                FROM auth_sessions
                WHERE last_verified_at < unixepoch('now') - 3600
                "#
        )
        .fetch_all(&mut conn)
        .await
        .expect("failed to query sessions");

        rows.iter()
            .map(|r| {
                (
                    r.session_id.clone(),
                    Authorized {
                        user_access_token: r.user_access_token.clone(),
                        app_id: r.app_id.clone(),
                        user_id: r.user_id.clone(),
                        expires_at: r
                            .expires_at
                            .map(|s| UNIX_EPOCH + Duration::from_secs(s as u64)),
                        last_verified_at: UNIX_EPOCH
                            + Duration::from_secs(r.last_verified_at as u64),
                    },
                )
            })
            .collect()
    };

    let count = sessions.len();
    if count == 0 {
        info!("no sessions needed verification");
    } else {
        info!(count, "sessions need verification");
    }

    for (session_id, mut auth_data) in sessions {
        match auth_data.verify(&client, &app_id, &app_secret).await {
            Ok(_) => {
                let expires_at = auth_data.expires_at.map(|t| {
                    t.duration_since(UNIX_EPOCH)
                        .map(|d| d.as_secs() as i64)
                        .unwrap_or(0)
                });
                let last_verified_at = auth_data
                    .last_verified_at
                    .duration_since(UNIX_EPOCH)
                    .expect("Time error")
                    .as_secs() as i64;

                if let Err(e) = sqlx::query!(
                    "UPDATE auth_sessions 
                         SET expires_at = ?1, last_verified_at = ?2
                         WHERE session_id = ?3",
                    expires_at,
                    last_verified_at,
                    session_id
                )
                .execute(&mut conn)
                .await
                {
                    tracing::error!(session_id, error = %e, "failed to update session");
                }
            }
            Err(e) => {
                tracing::warn!(session_id, error = %e, "session verification failed, deleting");

                if let Err(e) = sqlx::query!(
                    "DELETE FROM auth_sessions WHERE session_id = ?1",
                    session_id
                )
                .execute(&mut conn)
                .await
                {
                    tracing::error!(session_id, error = %e, "failed to delete invalid session");
                }
            }
        }
    }
}

/// Queries the database to get pending posts and check if they were published or failed and
/// updates the posts_issued table
#[instrument("checking on issued posts", skip(db_path, _app_secret))]
pub async fn post_maintenance(db_path: &str, app_id: &str, _app_secret: &str) {
    // let app_id = app_id.to_string();
    // let app_secret = app_secret.to_string();
    // let client = Client::new();
    //
    let mut conn = SqliteConnection::connect(db_path)
        .await
        .expect("Failed to connect to sqlite db");

    #[derive(Debug)]
    struct PostData {
        page_id: String,
        post_id: String,
    }

    let posts = sqlx::query_as!(
        PostData,
        r#"
        SELECT page_id AS "page_id!", post_id AS "post_id!" FROM posts_issued
        WHERE status = 'pending' 
        OR (status = 'failed' AND check_attempts < 5)
        ORDER BY created_at ASC;
    "#,
    )
    .fetch_all(&mut conn)
    .await
    .expect("Failed to query issued posts");

    let count = posts.len();
    if count == 0 {
        info!("no posts were in pending or failed");
    } else {
        info!(count, "posts need checking");
    }
}
