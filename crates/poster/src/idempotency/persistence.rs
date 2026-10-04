use axum::{body::Body, response::Response};
use http::StatusCode;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn get_saved_response(
    pool: &SqlitePool,
    idempotency_key: super::IdempotencyKey,
    user_id: Uuid,
) -> eyre::Result<Option<Response>> {
    let key = idempotency_key.as_ref().to_owned();
    let saved_response = sqlx::query!(
        r#"
        SELECT
            response_status_code,
            response_headers,
            response_body
        FROM idempotency
        WHERE
            user_id = $1 AND
            idempotency_key = $2
        "#,
        user_id,
        key
    )
    .fetch_optional(pool)
    .await?;

    if let Some(r) = saved_response {
        let status_code = StatusCode::from_u16(r.response_status_code.try_into()?)?;

        let mut response = Response::builder().status(status_code);

        for (name, value) in serde_json::from_str::<Vec<(String, Vec<u8>)>>(&r.response_headers)? {
            response = response.header(name, value);
        }

        let body = Body::from(r.response_body);
        let response = response.body(body)?;

        Ok(Some(response))
    } else {
        Ok(None)
    }
}
