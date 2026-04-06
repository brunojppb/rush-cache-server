use actix_web::body::BoxBody;
use actix_web::dev::{ServiceRequest, ServiceResponse};
use actix_web::middleware::Next;
use actix_web::{HttpMessage, HttpResponse, web};
use tracing::instrument;

use crate::app_settings::{AppSettings, TokenPermission};

/// Result of token validation: the permission level of the authenticated token.
#[derive(Debug, Clone)]
pub struct AuthenticatedToken {
    pub permission: TokenPermission,
    /// A safe-to-log prefix of the token (first 8 chars or less).
    pub token_prefix: String,
}

/// Actix-web middleware that validates Bearer tokens from the Authorization header.
/// Attaches an `AuthenticatedToken` to the request extensions on success.
#[instrument(name = "validate_bearer_token", skip_all)]
pub async fn validate_bearer_token(
    req: ServiceRequest,
    next: Next<BoxBody>,
) -> Result<ServiceResponse<BoxBody>, actix_web::Error> {
    let settings = req
        .app_data::<web::Data<AppSettings>>()
        .expect("AppSettings must be configured");

    let auth_header = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(header) => {
            // Case-insensitive "Bearer " prefix check
            if header.len() > 7 && header[..7].eq_ignore_ascii_case("bearer ") {
                Some(&header[7..])
            } else {
                None
            }
        }
        None => None,
    };

    let token = match token {
        Some(t) => t,
        None => {
            return Ok(req.into_response(
                HttpResponse::Unauthorized()
                    .json(serde_json::json!({"error": "Missing or malformed Authorization header"}))
                    .map_into_boxed_body(),
            ));
        }
    };

    match settings.token_store.resolve(token) {
        Some(permission) => {
            let token_prefix = if token.len() > 8 {
                format!("{}...", &token[..8])
            } else {
                token.to_string()
            };
            req.extensions_mut().insert(AuthenticatedToken {
                permission,
                token_prefix,
            });
            next.call(req).await
        }
        None => Ok(req.into_response(
            HttpResponse::Unauthorized()
                .json(serde_json::json!({"error": "Invalid token"}))
                .map_into_boxed_body(),
        )),
    }
}
