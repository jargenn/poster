use std::time::{Duration, UNIX_EPOCH};

use r2d2_sqlite::{
    rusqlite::{Connection, params},
};
use reqwest::Client;
use tracing::{info, instrument};

use facebook_graph_api::auth::Authorized;

#[instrument("cleaning and verifying sessions", skip(db_path,  app_secret))]
pub async fn maintain_sessions(db_path: &str, app_id: &str, app_secret: &str) {
    let db_path = db_path.to_string();
    let app_id = app_id.to_string();
    let app_secret = app_secret.to_string();
    let client = Client::new();

    let deleted = tokio::task::spawn_blocking({
        let db_path = db_path.clone();
        move || {
            let conn = Connection::open(&db_path)
                .expect("Couldn't open a connection to the database");
            conn.execute(
                "DELETE FROM auth_sessions WHERE expires_at < unixepoch('now')",
                [],
            )
            .expect("Failed to delete expired sessions")
        }
    })
    .await
    .expect("Task panicked");

    if deleted > 0 {
        info!(deleted, "expired sessions cleaned up");
    }

    let sessions: Vec<(String, Authorized)> = tokio::task::spawn_blocking({
        let db_path = db_path.clone();
        move || {
            let conn = Connection::open(&db_path)
                .expect("Couldn't open a connection to the database");
            
            let mut stmt = conn
                .prepare(
                    "SELECT session_id, user_access_token, app_id, user_id, expires_at, last_verified_at
                     FROM auth_sessions
                     WHERE last_verified_at < unixepoch('now') - 3600",
                )
                .expect("Failed to prepare statement");

            stmt.query_map([], |row| {
                let session_id: String = row.get(0)?;
                let expires_at: Option<i64> = row.get(4)?;
                let last_verified_at: i64 = row.get(5)?;
                
                Ok((
                    session_id,
                    Authorized {
                        user_access_token: row.get(1)?,
                        app_id: row.get(2)?,
                        user_id: row.get(3)?,
                        expires_at: expires_at.map(|s| UNIX_EPOCH + Duration::from_secs(s as u64)),
                        last_verified_at: UNIX_EPOCH + Duration::from_secs(last_verified_at as u64),
                    },
                ))
            })
            .expect("Failed to query sessions")
            .filter_map(Result::ok)
            .collect()
        }
    })
    .await
    .expect("Task panicked");

    let count = sessions.len();
    if count == 0 {
        info!("no sessions needed verification");
    } else {
        info!(count = sessions.len(), "sessions need verification");
    }

    for (session_id, mut auth) in sessions {
        match auth.verify(&client, &app_id, &app_secret).await {
            Ok(_) => {
                let session_id_clone = session_id.clone();
                let db_path_clone = db_path.clone();
                let auth_clone = auth.clone(); 
                
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    let conn = Connection::open(&db_path_clone)
                        .expect("Couldn't open a connection to the database");
                    
                    conn.execute(
                        "UPDATE auth_sessions 
                         SET expires_at = ?1, last_verified_at = ?2
                         WHERE session_id = ?3",
                        params![
                            auth_clone.expires_at.map(|t| t
                                .duration_since(UNIX_EPOCH)
                                .map(|d| d.as_secs() as i64)
                                .unwrap_or(0)),
                            auth_clone.last_verified_at
                                .duration_since(UNIX_EPOCH)
                                .expect("Time error")
                                .as_secs() as i64,
                            session_id_clone,
                        ],
                    )
                })
                .await
                {
                    tracing::error!(session_id, error = %e, "failed to update session");
                }
            }
            Err(e) => {
                tracing::warn!(session_id, error = %e, "session verification failed, deleting");
                
                let session_id_clone = session_id.clone();
                let db_path_clone = db_path.clone();
                
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    let conn = Connection::open(&db_path_clone)
                        .expect("Couldn't open a connection to the database");
                    
                    conn.execute(
                        "DELETE FROM auth_sessions WHERE session_id = ?1",
                        params![session_id_clone],
                    )
                })
                .await
                {
                    tracing::error!(session_id, error = %e, "failed to delete invalid session");
                }
            }
        }
    }
}
