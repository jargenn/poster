use color_eyre::owo_colors::OwoColorize;
use eyre::Result;
use facebook_graph_api::auth::Authorized;
use sqlx::PgConnection;
use time::OffsetDateTime;
use tracing::{debug, error, info, instrument};
use uuid::Uuid;

#[instrument("Storing session data",
    skip(conn, auth),
    fields(
        user_id,
        fb_user_id = %auth.user_id,
        fb_app_id = %auth.app_id
    )
)]
pub async fn store_oauth_data(
    conn: &mut PgConnection,
    user_id: &Uuid,
    auth: &Authorized,
) -> Result<()> {
    info!("storing auth session");

    let expires_at = auth.expires_at.map(OffsetDateTime::from);

    sqlx::query!(
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
    .execute(conn)
    .await
    .expect("Failed to insert new sessions in the database");

    Ok(())
}

#[instrument("Searching for the session access_token in the database"
    skip(conn),
    fields(user_id= %user_id.bold())
)]
pub async fn load_session(conn: &mut PgConnection, user_id: &Uuid) -> Result<Option<Authorized>> {
    debug!("searching for access_token in the database");

    #[derive(Debug, sqlx::FromRow)]
    struct SessionRow {
        fb_user_access_token: String,
        fb_app_id: String,
        fb_user_id: String,
        expires_at: Option<OffsetDateTime>,
        last_verified_at: OffsetDateTime,
    }

    let row = match sqlx::query_as!(
        SessionRow,
        "
            SELECT
                fb_user_access_token,
                fb_app_id,
                fb_user_id,
                expires_at,
                last_verified_at
            FROM facebook_auth_data
            WHERE user_id = $1
            ",
        user_id,
    )
    .fetch_optional(conn)
    .await
    {
        Ok(opt) => match opt {
            None => {
                debug!(%user_id,"no session found for access_token");
                return Ok(None);
            }
            Some(r) => {
                debug!("session found");
                r
            }
        },
        Err(e) => {
            error!(%e,"Error while search for session in facebook_auth_data table");
            return Err(eyre::eyre!(
                "Error while search for session in facebook_auth_data table: {e}"
            ));
        }
    };

    Ok(Some(Authorized {
        user_access_token: row.fb_user_access_token,
        app_id: row.fb_app_id,
        user_id: row.fb_user_id,
        expires_at: row.expires_at.map(Into::into),
        last_verified_at: row.last_verified_at.into(),
    }))
}
