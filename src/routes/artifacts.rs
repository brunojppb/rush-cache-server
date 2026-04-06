use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use futures::StreamExt;
use tracing::instrument;

use crate::app_settings::{AppSettings, TokenPermission};
use crate::auth::bearer_token::AuthenticatedToken;
use crate::storage::Storage;

#[derive(Debug, serde::Deserialize)]
pub struct ArtifactPath {
    pub cache_id: String,
}

/// GET /artifacts/{cache_id} — retrieve a cached build artifact.
#[instrument(name = "get_artifact", skip(storage), fields(cache_id = %path.cache_id))]
pub async fn get_artifact(
    path: web::Path<ArtifactPath>,
    storage: web::Data<Storage>,
    _settings: web::Data<AppSettings>,
) -> HttpResponse {
    let cache_id = &path.cache_id;

    match storage.get_file(cache_id).await {
        Ok(Some(data)) => HttpResponse::Ok()
            .content_type("application/octet-stream")
            .body(data),
        Ok(None) => HttpResponse::NotFound().json(serde_json::json!({"error": "Cache miss"})),
        Err(e) => {
            tracing::error!(error = %e, cache_id = %cache_id, "Failed to get artifact from S3");
            map_storage_error(e)
        }
    }
}

/// PUT /artifacts/{cache_id} — store a build artifact.
#[instrument(name = "put_artifact", skip(storage, body), fields(cache_id = %path.cache_id))]
pub async fn put_artifact(
    path: web::Path<ArtifactPath>,
    req: HttpRequest,
    storage: web::Data<Storage>,
    _settings: web::Data<AppSettings>,
    mut body: web::Payload,
) -> HttpResponse {
    // Check write permission
    let auth = req.extensions().get::<AuthenticatedToken>().cloned();
    if let Some(ref auth) = auth
        && auth.permission == TokenPermission::ReadOnly
    {
        return HttpResponse::Forbidden()
            .json(serde_json::json!({"error": "Token does not have write permission"}));
    }

    let cache_id = &path.cache_id;

    // Collect the body stream into bytes
    let mut bytes = web::BytesMut::new();
    while let Some(chunk) = body.next().await {
        match chunk {
            Ok(data) => bytes.extend_from_slice(&data),
            Err(e) => {
                tracing::error!(error = %e, "Failed to read request body");
                return HttpResponse::InternalServerError()
                    .json(serde_json::json!({"error": "Failed to read request body"}));
            }
        }
    }

    match storage.put_file(cache_id, bytes.freeze()).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!(error = %e, cache_id = %cache_id, "Failed to put artifact to S3");
            map_storage_error(e)
        }
    }
}

fn map_storage_error(err: crate::storage::StorageError) -> HttpResponse {
    match err {
        crate::storage::StorageError::NotFound => {
            HttpResponse::NotFound().json(serde_json::json!({"error": "Cache miss"}))
        }
        crate::storage::StorageError::AccessDenied => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": "Internal storage error"})),
        crate::storage::StorageError::Unavailable(msg) => HttpResponse::ServiceUnavailable().json(
            serde_json::json!({"error": format!("Storage temporarily unavailable: {}", msg)}),
        ),
        crate::storage::StorageError::Other(msg) => HttpResponse::InternalServerError()
            .json(serde_json::json!({"error": format!("Internal error: {}", msg)})),
    }
}
