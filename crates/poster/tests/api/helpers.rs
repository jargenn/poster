use std::sync::LazyLock;

use poster::{
    configuration::{AppConfig, DatabaseSettings},
    server::{Application, get_connection_pool},
    telemetry::init_tracing,
};
use sqlx::{Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;

static TRACING: LazyLock<()> = LazyLock::new(|| {
    init_tracing();
});

pub struct TestApp {
    pub address: String,
    pub pool: PgPool,
}

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    let configuration = {
        let mut c = AppConfig::get_config().expect("Failed to read configuration.");
        // Use a different database for each test case
        c.database.database_name = Uuid::new_v4().to_string();
        // Use a random OS port
        c.port = 0;
        c
    };
    configure_database(&configuration.database);

    let app = Application::build(configuration.clone())
        .await
        .expect("Failed to build application");
    let address = format!("http://127.0.0.1:{}", app.port());

    let _ = tokio::spawn(app.run_until_stopped());

    TestApp {
        address: address,
        pool: get_connection_pool(&configuration.database),
    }
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
