use actix_web::{HttpMessage, HttpRequest, HttpResponse, web};
use futures::StreamExt;
use tokio_util::io::StreamReader;
use tracing::instrument;

use crate::app_settings::{AppSettings, TokenPermission};
use crate::auth::bearer_token::AuthenticatedToken;
use crate::storage::Storage;

#[derive(Debug, serde::Deserialize)]
pub struct ArtifactPath {
    pub cache_id: String,
}

/// GET /artifacts/{cache_id} — stream a cached build artifact from S3.
#[instrument(name = "get_artifact", skip(storage), fields(cache_id = %path.cache_id))]
pub async fn get_artifact(
    path: web::Path<ArtifactPath>,
    storage: web::Data<Storage>,
    _settings: web::Data<AppSettings>,
) -> HttpResponse {
    let cache_id = &path.cache_id;

    let Some(response) = storage.get_file(cache_id).await else {
        return HttpResponse::NotFound().json(serde_json::json!({"error": "Cache miss"}));
    };

    // Stream S3 response bytes directly to the HTTP client
    let stream = response.bytes.map(|maybe_chunk| match maybe_chunk {
        Ok(bytes) => Ok::<_, actix_web::error::Error>(bytes),
        Err(error) => {
            tracing::error!(error = %error, "Chunk stream error");
            Err(actix_web::error::ErrorInternalServerError(
                "Error while streaming artifact",
            ))
        }
    });

    HttpResponse::Ok()
        .content_type("application/octet-stream")
        .streaming(stream)
}

/// PUT /artifacts/{cache_id} — stream a build artifact to S3.
#[instrument(name = "put_artifact", skip(storage, body), fields(cache_id = %path.cache_id))]
pub async fn put_artifact(
    path: web::Path<ArtifactPath>,
    req: HttpRequest,
    storage: web::Data<Storage>,
    _settings: web::Data<AppSettings>,
    body: web::Payload,
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

    // Convert the Actix Payload stream into an AsyncRead for streaming to S3
    let io_stream = body.map(|chunk| chunk.map_err(std::io::Error::other));
    let mut reader = StreamReader::new(io_stream);

    match storage.put_file_stream(cache_id, &mut reader).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({"success": true})),
        Err(e) => {
            tracing::error!(error = %e, cache_id = %cache_id, "Failed to put artifact to S3");
            HttpResponse::InternalServerError()
                .json(serde_json::json!({"error": "Failed to store artifact"}))
        }
    }
}
