// TODO: Take in consideration that fetch_all loads everything it finds to memory, so in production
// I should limit the number of elements it returns.

use reqwest::Client;
use sqlx::{Connection, PgConnection};
use time::OffsetDateTime;
use tracing::{info, instrument};

use facebook_graph_api::auth::Authorized;

#[instrument(
    "Periodic job maintaining session freshness",
    skip(db_path, app_secret)
)]
pub async fn session_maintenance(db_path: &str, app_id: &str, app_secret: &str) {
    let app_id = app_id.to_string();
    let app_secret = app_secret.to_string();
    let client = Client::new();

    let mut conn = PgConnection::connect(db_path)
        .await
        .expect("Failed to open a connection to the sqlite db");

    let deleted = sqlx::query!(
        "DELETE FROM auth_sessions WHERE expires_at IS NOT NULL AND expires_at < NOW()"
    )
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
        expires_at: Option<OffsetDateTime>,
        last_verified_at: OffsetDateTime,
    }

    let sessions: Vec<(String, Authorized)> = {
        let rows = sqlx::query_as!(
            AuthRows,
            r#"
                SELECT
                    session_id,        
                    user_access_token, 
                    app_id,            
                    user_id,
                    expires_at,
                    last_verified_at
                FROM auth_sessions
                WHERE last_verified_at < NOW() - INTERVAL '1 hour'
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
                        expires_at: r.expires_at.map(Into::into),
                        last_verified_at: r.last_verified_at.into(),
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
            Ok(()) => {
                let expires_at: Option<OffsetDateTime> = auth_data.expires_at.map(Into::into);
                let last_verified_at: OffsetDateTime = auth_data.last_verified_at.into();

                if let Err(e) = sqlx::query!(
                    "UPDATE auth_sessions 
                         SET expires_at = $1, last_verified_at = $2
                         WHERE session_id = $3",
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
                    "DELETE FROM auth_sessions WHERE session_id = $1",
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

// /// Queries the database to get pending posts and check if they were published or failed and
// /// updates the posts_issued table
// #[instrument("checking on issued posts", skip(db_path, _app_secret))]
// pub async fn post_maintenance(db_path: &str, app_id: &str, _app_secret: &str) {
//     // let app_id = app_id.to_string();
//     // let app_secret = app_secret.to_string();
//     // let client = Client::new();
//     //
//     let mut conn = PgConnection::connect(db_path)
//         .await
//         .expect("Failed to connect to sqlite db");

//     #[derive(Debug)]
//     struct PostData {
//         page_id: String,
//         post_id: String,
//     }

//     let posts = sqlx::query_as!(
//         PostData,
//         r#"
//         SELECT page_id AS "page_id!", post_id AS "post_id!" FROM posts_issued
//         WHERE status = 'pending'
//         OR (status = 'failed' AND check_attempts < 5)
//         ORDER BY created_at ASC;
//     "#,
//     )
//     .fetch_all(&mut conn)
//     .await
//     .expect("Failed to query issued posts");

//     let count = posts.len();
//     if count == 0 {
//         info!("no posts were in pending or failed");
//     } else {
//         info!(count, "posts need checking");
//     }
// }
// // Get all posts for a specific page
// sqlx::query_as!(
//     ScheduledPost,
//     "SELECT * FROM scheduled_posts
//      WHERE page_id = $1
//      ORDER BY scheduled_for DESC",
//     page_id
// )
// .fetch_all(conn)
// .await?;

// // Get pending posts for a user across all their pages
// sqlx::query_as!(
//     ScheduledPost,
//     "SELECT * FROM scheduled_posts
//      WHERE user_id = $1
//      AND status = 'pending'
//      ORDER BY scheduled_for ASC",
//     user_id
// )
// .fetch_all(conn)
// .await?;
