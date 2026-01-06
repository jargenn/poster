// TODO: Take in consideration that fetch_all loads everything it finds to memory, so in production
// I should limit the number of elements it returns.

// use moka::future::Cache;
// use reqwest::Client;
// use sqlx::PgConnection;
// use std::collections::HashMap;
// use time::OffsetDateTime;
// use tracing::{info, instrument};

// use facebook_graph_api::auth::Authorized;

// #[instrument("Periodic job maintaining session freshness", skip(conn, auth_cache))]
// pub async fn session_maintenance(conn: &mut PgConnection, auth_cache: &Cache<String, Authorized>) {
//     let client = Client::new();

//     let deleted = sqlx::query!(
//         "DELETE FROM facebook_auth_data WHERE expires_at IS NOT NULL AND expires_at < NOW()"
//     )
//     .execute(&mut *conn)
//     .await
//     .expect("Delete session query failed")
//     .rows_affected();

//     if deleted > 0 {
//         info!(deleted, "expired sessions cleaned up");
//     }

//     #[derive(Debug)]
//     struct AuthRows {
//         session_id: String,
//         user_access_token: String,
//         app_id: String,
//         user_id: String,
//         expires_at: Option<OffsetDateTime>,
//         last_verified_at: OffsetDateTime,
//     }

//     let sessions: Vec<(String, Authorized)> = {
//         let rows = sqlx::query_as!(
//             AuthRows,
//             r#"
//                 SELECT
//                     session_id,
//                     user_access_token,
//                     app_id,
//                     user_id,
//                     expires_at,
//                     last_verified_at
//                 FROM facebook_auth_data
//                 WHERE last_verified_at < NOW() - INTERVAL '1 hour'
//                 "#
//         )
//         .fetch_all(&mut *conn)
//         .await
//         .expect("failed to query sessions");

//         rows.iter()
//             .map(|r| {
//                 (
//                     r.session_id.clone(),
//                     Authorized {
//                         user_access_token: r.user_access_token.clone(),
//                         app_id: r.app_id.clone(),
//                         user_id: r.user_id.clone(),
//                         expires_at: r.expires_at.map(Into::into),
//                         last_verified_at: r.last_verified_at.into(),
//                     },
//                 )
//             })
//             .collect()
//     };

//     let count = sessions.len();
//     if count == 0 {
//         info!("no sessions needed verification");
//     } else {
//         info!(count, "sessions need verification");
//     }

//     let user_configs: HashMap<String, String> =
//         sqlx::query!("SELECT app_id, app_secret FROM user_configs;")
//             .fetch_all(&mut *conn)
//             .await
//             .expect("Failed to query the db")
//             .into_iter()
//             .map(|row| (row.app_id, row.app_secret))
//             .collect();

//     for (session_id, mut auth_data) in sessions {
//         let app_id = auth_data.app_id.clone();

//         let Some(app_secret) = user_configs.get(&app_id) else {
//             tracing::warn!(
//                 session_id,
//                 app_id = %auth_data.app_id,
//                 "no matching app config found, deleting session"
//             );

//             let _ = sqlx::query!(
//                 "DELETE FROM facebook_auth_data WHERE session_id = $1",
//                 session_id
//             )
//             .execute(&mut *conn)
//             .await;

//             tracing::debug!(%session_id,"Invalidating key in the auth cache");
//             auth_cache.invalidate(&session_id).await;

//             continue;
//         };

//         match auth_data.verify(&client, &app_id, app_secret).await {
//             Ok(()) => {
//                 let expires_at: Option<OffsetDateTime> = auth_data.expires_at.map(Into::into);
//                 let last_verified_at: OffsetDateTime = auth_data.last_verified_at.into();

//                 if let Err(e) = sqlx::query!(
//                     "UPDATE facebook_auth_data
//                          SET expires_at = $1, last_verified_at = $2
//                          WHERE session_id = $3",
//                     expires_at,
//                     last_verified_at,
//                     session_id
//                 )
//                 .execute(&mut *conn)
//                 .await
//                 {
//                     tracing::error!(session_id, error = %e, "failed to update session");
//                 }

//                 tracing::debug!(%session_id, "Inserting/Updating key in the auth cache");
//                 auth_cache.insert(session_id, auth_data).await;
//             }
//             Err(e) => {
//                 tracing::warn!(session_id, error = %e, "session verification failed, deleting");

//                 if let Err(e) = sqlx::query!(
//                     "DELETE FROM facebook_auth_data WHERE session_id = $1",
//                     session_id
//                 )
//                 .execute(&mut *conn)
//                 .await
//                 {
//                     tracing::error!(session_id, error = %e, "failed to delete invalid session");
//                 }

//                 tracing::debug!(%session_id,"Invalidating key in the auth cache");
//                 auth_cache.invalidate(&session_id).await;
//             }
//         }
//     }
// }
