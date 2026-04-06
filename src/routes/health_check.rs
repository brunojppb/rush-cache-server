use actix_web::HttpResponse;
use tracing::instrument;

#[instrument(name = "health_check")]
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({"status": "healthy"}))
}
