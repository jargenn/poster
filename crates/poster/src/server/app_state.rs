// #[instrument(skip(self))]
// pub async fn warmup_cache(&self) -> Result<(), Error> {
//     let mut conn = self.pool.acquire().await.map_err(Error::Database)?;

//     #[derive(Debug)]
//     struct UserConfigRow {
//         id: Uuid,
//         app_id: String,
//         app_secret: String,
//         app_config_id: String,
//         redirect_url: String,
//     }

//     let rows = sqlx::query_as!(
//         UserConfigRow,
//         r#"
// SELECT
//     id,
//     app_id,
//     app_secret,
//     app_config_id,
//     redirect_url
// FROM user_configs
// "#
//     )
//     .fetch_all(&mut *conn)
//     .await
//     .map_err(Error::Database)?;

//     let count = rows.len();
//     debug!(count, "User configs found in the database");

//     for r in rows {
//         self.user_config
//             .insert(
//                 r.id.to_string(),
//                 ConfigData {
//                     app_id: r.app_id,
//                     app_secret: r.app_secret,
//                     app_config_id: r.app_config_id,
//                     redirect_url: RedirectUri::try_from(r.redirect_url)
//                         .expect("Invalid URL read from the database"),
//                 },
//             )
//             .await;
//     }

//     debug!(count, "User configs inserted in cache");

//     Ok(())
// }
// }
