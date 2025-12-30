use axum::response::IntoResponse;
use http::StatusCode;
use tracing::instrument;

#[instrument("Check for service health")]
pub async fn health_check() -> impl IntoResponse {
    (StatusCode::OK, "System is ready")
}
