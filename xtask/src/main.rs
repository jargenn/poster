use std::env::{self, args};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

pub fn setup_database() -> Result<(), Box<dyn std::error::Error>> {
    if !command_exists("psql") {
        eprintln!("Error: psql is not installed.");
        return Err("psql not found".into());
    }

    if !command_exists("sqlx") {
        eprintln!("Error: sqlx is not installed.");
        eprintln!("Use:");
        eprintln!(
            "    cargo install --version=0.8.6 sqlx-cli --no-default-features --features postgres,rustls"
        );
        eprintln!("to install it.");
        return Err("sqlx not found".into());
    }

    let db_user = env::var("POSTGRES_USER").unwrap_or_else(|_| "postgres".to_string());
    let db_password = env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| "password".to_string());
    let db_name = env::var("POSTGRES_DB").unwrap_or_else(|_| "poster".to_string());
    let db_port = env::var("POSTGRES_PORT").unwrap_or_else(|_| "5432".to_string());
    let migration_path =
        env::var("MIGRATION_PATH").unwrap_or_else(|_| "./crates/poster/migrations".to_string());

    if env::var("SKIP_DOCKER").is_err() {
        println!("Starting PostgreSQL Docker container...");

        let status = Command::new("docker")
            .args([
                "run",
                "-e",
                &format!("POSTGRES_USER={}", db_user),
                "-e",
                &format!("POSTGRES_PASSWORD={}", db_password),
                "-e",
                &format!("POSTGRES_DB={}", db_name),
                "-p",
                &format!("{}:5432", db_port),
                "-d",
                "postgres",
                "postgres",
                "-N",
                "1000",
            ])
            .status()?;

        if !status.success() {
            return Err("Failed to start Docker container".into());
        }
    }

    unsafe { env::set_var("PGPASSWORD", &db_password) };

    println!("Waiting for Postgres to be ready...");
    loop {
        let output = Command::new("psql")
            .args([
                "-h",
                "localhost",
                "-U",
                &db_user,
                "-p",
                &db_port,
                "-d",
                "postgres",
                "-c",
                r"\q",
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();

        match output {
            Ok(status) if status.success() => {
                println!(
                    "Postgres is up and running on port {} - running migrations now!",
                    db_port
                );
                break;
            }
            _ => {
                eprintln!("Postgres is still unavailable - sleeping");
                thread::sleep(Duration::from_secs(1));
            }
        }
    }

    let database_url = format!(
        "postgres://{}:{}@localhost:{}/{}",
        db_user, db_password, db_port, db_name
    );
    unsafe { env::set_var("DATABASE_URL", &database_url) };

    let status = Command::new("sqlx").args(["database", "create"]).status()?;

    if !status.success() {
        return Err("Failed to create database".into());
    }

    let status = Command::new("sqlx")
        .args(["migrate", "run", "--source", &migration_path])
        .status()?;

    if !status.success() {
        return Err("Failed to run migrations".into());
    }

    println!("Postgres has been migrated, ready to go!");
    Ok(())
}

/// Helper function to check if a command exists
fn command_exists(cmd: &str) -> bool {
    Command::new("sh")
        .arg("-c")
        .arg(format!("command -v {}", cmd))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut params = args();
    params.next();

    match params.next() {
        None => println!("No command given"),
        Some(cmd) => match cmd.as_str() {
            "start_db" => setup_database()?,
            _ => println!("Unknown command {cmd}"),
        },
    };

    Ok(())
}
