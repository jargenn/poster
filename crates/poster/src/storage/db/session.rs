use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use facebook_graph_api::auth::Authorized;
use sqlx::PgConnection;
use time::OffsetDateTime;
use tracing::{debug, error, info, instrument};

use crate::cookies::SessionId;

#[instrument("Storing session data",
    skip(conn, auth),
    fields(
        session_id = %session_id,
        user_id = %auth.user_id,
        app_id = %auth.app_id
    )
)]
pub async fn store_session(
    conn: &mut PgConnection,
    session_id: &SessionId,
    auth: &Authorized,
) -> Result<()> {
    info!("storing auth session");

    let session_id = session_id.to_string();
    let expires_at = auth.expires_at.map(OffsetDateTime::from);

    let inserted = sqlx::query!(
        "INSERT INTO auth_sessions (
                session_id,
                user_access_token,
                app_id,
                user_id,
                expires_at
            )
            VALUES ($1, $2, $3, $4, $5)
            ",
        session_id,
        auth.user_access_token,
        auth.app_id,
        auth.user_id,
        expires_at
    )
    .execute(conn)
    .await
    .expect("Failed to insert new sessions in the database")
    .rows_affected();

    info!("{inserted} session_data in the database");
    Ok(())
}

#[instrument("Searching for the session_id in the database"
    skip(conn),
    fields(session_id = %session_id.bold())
)]
pub async fn load_session(
    conn: &mut PgConnection,
    session_id: &SessionId,
) -> Result<Option<Authorized>> {
    debug!("searching for session_id in the database");

    #[derive(Debug, sqlx::FromRow)]
    struct SessionRow {
        user_access_token: String,
        app_id: String,
        user_id: String,
        expires_at: Option<OffsetDateTime>,
        last_verified_at: OffsetDateTime,
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
            WHERE session_id = $1
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
        expires_at: row.expires_at.map(Into::into),
        last_verified_at: row.last_verified_at.into(),
    }))
}
