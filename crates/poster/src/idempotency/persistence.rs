use axum::{body::Body, response::Response};
use http::StatusCode;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, sqlx::Type)]
#[sqlx(type_name = "header_pair")]
struct HeaderPairRecord {
    name: String,
    value: Vec<u8>,
}

pub async fn get_saved_response(
    pool: &PgPool,
    idempotency_key: super::IdempotencyKey,
    user_id: Uuid,
) -> eyre::Result<Option<Response>> {
    let saved_response = sqlx::query!(
        r#"
        SELECT
            response_status_code,
            response_headers as "response_headers: Vec<HeaderPairRecord>",
            response_body
        FROM idempotency
        WHERE
            user_id = $1 AND
            idempotency_key = $2
        "#,
        user_id,
        idempotency_key.as_ref()
    )
    .fetch_optional(pool)
    .await?;

    if let Some(r) = saved_response {
        let status_code = StatusCode::from_u16(r.response_status_code.try_into()?)?;

        let mut response = Response::builder().status(status_code);

        for HeaderPairRecord { name, value } in r.response_headers {
            response = response.header(name, value);
        }

        let body = Body::from(r.response_body);
        let response = response.body(body)?;

        Ok(Some(response))
    } else {
        Ok(None)
    }
}
