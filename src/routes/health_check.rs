use actix_web::HttpResponse;
use tracing::instrument;

/// Shared by the route registration and the root span builder so the two
/// cannot drift apart.
pub const HEALTH_CHECK_PATH: &str = "/health";

#[instrument(name = "health_check")]
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "healthy"}))
}
