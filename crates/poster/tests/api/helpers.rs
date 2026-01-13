use argon2::{
    Algorithm, Argon2, Params, PasswordHasher, Version,
    password_hash::{SaltString, rand_core},
};
use expect_test::Expect;
use http::{HeaderMap, StatusCode};
use poster::{
    configuration::{AppConfig, DatabaseSettings, StorageBackend},
    server::{Application, get_connection_pool},
    telemetry::init_tracing,
};
use reqwest::Client;
use sqlx::{Connection, Executor, PgConnection, PgPool};
use std::{
    sync::LazyLock,
    time::{Duration, SystemTime},
};
use tempfile::TempDir;
use time::OffsetDateTime;
use uuid::Uuid;
use wiremock::MockServer;

static TRACING: LazyLock<()> = LazyLock::new(|| {
    init_tracing();
});

pub fn check<T: std::fmt::Debug>(body: T, expect: Expect) {
    expect.assert_debug_eq(&body);
}

pub struct TestUser {
    pub user_id: Uuid,
    pub username: String,
    pub password: String,
}

impl TestUser {
    pub fn generate() -> Self {
        Self {
            user_id: Uuid::new_v4(),
            username: Uuid::new_v4().to_string(),
            password: Uuid::new_v4().to_string(),
        }
    }

    async fn store(&self, pool: &PgPool) {
        let salt = SaltString::generate(&mut rand_core::OsRng);
        let password_hash = Argon2::new(
            Algorithm::Argon2id,
            Version::V0x13,
            Params::new(15000, 2, 1, None).unwrap(),
        )
        .hash_password(self.password.as_bytes(), &salt)
        .unwrap()
        .to_string();

        sqlx::query!(
            "INSERT INTO users (user_id, username, password_hash)
                VALUES ($1, $2, $3)",
            self.user_id,
            self.username,
            password_hash,
        )
        .execute(pool)
        .await
        .expect("Failed to store test user.");
    }
}

pub struct TestApp {
    pub address: String,
    pub pool: PgPool,
    pub facebook_server: MockServer,
    pub test_user: TestUser,
    pub client: Client,
    __temp_dir: TempDir,
}

impl TestApp {
    pub async fn submit_post(
        &self,
        headers: &HeaderMap,
        body: &serde_json::Value,
    ) -> reqwest::Response {
        self.client
            .post(format!("{}/facebook/page_api/feed/1", self.address))
            .headers(headers.clone())
            .basic_auth(&self.test_user.username, Some(&self.test_user.password))
            .json(body)
            .send()
            .await
            .expect("Failed to send the request to the server during testing")
    }

    pub async fn post_login<B>(&self, body: &B) -> reqwest::Response
    where
        B: serde::Serialize,
    {
        self.client
            .post(format!("{}/login", self.address))
            .form(body)
            .send()
            .await
            .expect("Failed to send the request to the server during testing")
    }

    pub async fn post_config(&self, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .post(format!("{}/config", self.address))
            .json(body)
            .send()
            .await
            .expect("Failed to send the request to the server during testing")
    }

    pub async fn seed_logging(&self) {
        let login_body = serde_json::json!({
            "username": self.test_user.username,
            "password": self.test_user.password,
        });

        let response = self.post_login(&login_body).await;

        assert_eq!(response.status(), StatusCode::OK);
    }

    // pub async fn test_session(&self) -> String {
    //     let row = sqlx::query!("SELECT user_id FROM facebook_auth_data LIMIT 1",)
    //         .fetch_one(&self.pool)
    //         .await
    //         .expect("Failed to create test users.");

    //     row.user_id.to_string()
    // }
}

pub async fn setup_session(pool: &PgPool, user_id: Uuid) {
    let auth = AuthTestData::default();
    let expires_at = auth.expires_at.map(OffsetDateTime::from);

    let mut conn = pool
        .acquire()
        .await
        .expect("Failed to get a connection to the DB");

    let inserted = sqlx::query!(
        "INSERT INTO facebook_auth_data (
                user_id,
                fb_user_access_token,
                fb_app_id,
                fb_user_id,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5)
            ",
        user_id,
        auth.user_access_token,
        auth.app_id,
        auth.user_id,
        expires_at
    )
    .execute(&mut *conn)
    .await
    .expect("Failed to insert new sessions in the database")
    .rows_affected();

    assert_eq!(inserted, 1);
}

/// TODO: Work on supporting all te combinations of StorageBackend
/// Spaws a test app where its StorageBackend is Local
pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    let facebook_server = MockServer::start().await;
    let tmp = TempDir::new().expect("Failed to create a temp dir");
    let configuration = {
        let mut c = AppConfig::get_config().expect("Failed to read configuration.");
        // Use a different database for each test case
        c.database.database_name = Uuid::new_v4().to_string();
        // Use a random OS port
        c.port = 0;
        c.media_settings.storage_settings = StorageBackend::Local {
            base_path: tmp.path().to_path_buf(),
            base_url: None,
        };

        c.facebook_uri = facebook_server.uri();

        c
    };
    configure_database(&configuration.database).await;

    let client = Client::builder().cookie_store(true).build().unwrap();

    let pool = get_connection_pool(&configuration.database);

    let app = Application::build(configuration.clone())
        .await
        .expect("Failed to build application");

    let address = format!("http://127.0.0.1:{}", app.port());

    let _ = tokio::spawn(app.run_until_stopped());

    let test_app = TestApp {
        address: address,
        pool,
        facebook_server,
        __temp_dir: tmp,
        test_user: TestUser::generate(),
        client,
    };
    test_app.test_user.store(&test_app.pool).await;
    setup_session(&test_app.pool, test_app.test_user.user_id).await;

    test_app
}

async fn configure_database(config: &DatabaseSettings) {
    let mut connection = PgConnection::connect_with(&config.without_db())
        .await
        .expect("Failed to connect to Postgres.");

    connection
        .execute(format!(r#"CREATE DATABASE "{}";"#, config.database_name).as_str())
        .await
        .expect("Failed to create database.");

    let connection_pool = PgPool::connect_with(config.with_db())
        .await
        .expect("Failed to connect to Postgress.");

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate the database");
}

struct AuthTestData {
    user_access_token: String,
    app_id: String,
    user_id: String,
    expires_at: Option<SystemTime>,
}

impl Default for AuthTestData {
    fn default() -> Self {
        Self {
            user_access_token: String::from("test_user_access_token"),
            app_id: String::from("test_app_id"),
            user_id: String::from("test_user_id"),
            expires_at: Some(SystemTime::now() + Duration::from_secs(60)),
        }
    }
}
