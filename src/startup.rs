use actix_web::dev::Server;
use actix_web::middleware::from_fn;
use actix_web::{App, HttpServer, web};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

use crate::app_settings::AppSettings;
use crate::auth::bearer_token::validate_bearer_token;
use crate::routes::artifacts::{get_artifact, put_artifact};
use crate::routes::health_check::health_check;
use crate::storage::Storage;

/// Build and return the Actix-web server (without starting it).
pub fn run(listener: TcpListener, settings: AppSettings) -> Result<Server, std::io::Error> {
    let storage = Storage::new(&settings);
    let storage = web::Data::new(storage);
    let settings = web::Data::new(settings.clone());
    let max_body_size = settings.max_body_size;

    let server = HttpServer::new(move || {
        App::new()
            .wrap(TracingLogger::default())
            // Health check — no auth
            .route("/health", web::get().to(health_check))
            // Artifact routes — behind auth middleware
            .service(
                web::scope("/artifacts")
                    .wrap(from_fn(validate_bearer_token))
                    .route("/{cache_id}", web::get().to(get_artifact))
                    .route("/{cache_id}", web::put().to(put_artifact)),
            )
            .app_data(storage.clone())
            .app_data(settings.clone())
            .app_data(web::PayloadConfig::new(max_body_size))
    })
    .listen(listener)?
    .run();

    Ok(server)
}
