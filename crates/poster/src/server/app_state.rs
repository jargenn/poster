use std::time::Duration;

use facebook_graph_api::auth::{Authorized, RedirectUri};
use moka::future::{Cache, CacheBuilder};
use sqlx::{PgPool, postgres::PgPoolOptions};
use tracing::{debug, instrument};
use uuid::Uuid;

use crate::{
    configuration::{AppConfig, ConfigData},
    error::Error,
};

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: AppConfig,
    pub user_config: Cache<String, ConfigData>,
    pub auth_data: Cache<String, Authorized>,
    pub pool: PgPool,
}

impl AppState {
    pub fn new(config: AppConfig) -> Self {
        // TinyLFU cache with a 1 hour TTL.
        let user_config = CacheBuilder::new(100)
            .name(&config.caches.user_config.name)
            .time_to_live(Duration::from_secs(config.caches.user_config.ttl))
            .build();

        let auth_data = CacheBuilder::new(100)
            .name(&config.caches.auth_data.name)
            .time_to_live(Duration::from_secs(config.caches.auth_data.ttl))
            .build();

        let pool = PgPoolOptions::new()
            .max_connections(10)
            .acquire_timeout(Duration::from_secs(2))
            .connect_lazy(&config.database.connection_string())
            .expect("Failed to connect to the DB");

        Self {
            config,
            user_config,
            auth_data,
            pool,
        }
    }

    #[instrument(skip(self))]
    pub async fn warmup_cache(&self) -> Result<(), Error> {
        let mut conn = self.pool.acquire().await.map_err(Error::Database)?;

        #[derive(Debug)]
        struct UserConfigRow {
            id: Uuid,
            app_id: String,
            app_secret: String,
            app_config_id: String,
            redirect_url: String,
        }

        let rows = sqlx::query_as!(
            UserConfigRow,
            r#"
    SELECT
        id,
        app_id,
        app_secret,
        app_config_id,
        redirect_url
    FROM user_configs
    "#
        )
        .fetch_all(&mut *conn)
        .await
        .map_err(Error::Database)?;

        let count = rows.len();
        debug!(count, "User configs found in the database");

        for r in rows {
            self.user_config
                .insert(
                    r.id.to_string(),
                    ConfigData {
                        app_id: r.app_id,
                        app_secret: r.app_secret,
                        app_config_id: r.app_config_id,
                        redirect_url: RedirectUri::try_from(r.redirect_url)
                            .expect("Invalid URL read from the database"),
                    },
                )
                .await;
        }

        debug!(count, "User configs inserted in cache");

        Ok(())
    }
}
