use rush_cache_server::app_settings::AppSettings;
use rush_cache_server::startup::run;
use rush_cache_server::telemetry::{
    get_telemetry_subscriber, init_system_metrics, init_telemetry_subscriber,
};
use std::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let settings = AppSettings::from_env();

    let subscriber = get_telemetry_subscriber(settings.log_level.clone(), std::io::stdout);
    init_telemetry_subscriber(subscriber);

    let _system_metrics = init_system_metrics();

    let address = format!("{}:{}", settings.host, settings.port);
    let listener = TcpListener::bind(&address)?;
    tracing::info!("Starting server on {}", address);

    run(listener, settings)?.await
}
