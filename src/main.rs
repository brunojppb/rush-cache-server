use rush_cache_server::app_settings::AppSettings;
use rush_cache_server::startup::run;
use rush_cache_server::telemetry::{SystemMetrics, init_meter_provider, init_telemetry};
use std::net::TcpListener;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    dotenv::dotenv().ok();

    let settings = AppSettings::from_env();

    init_telemetry(&settings.log_level, settings.logs_directory.as_deref());

    let _meter_provider = init_meter_provider();

    if _meter_provider.is_some() {
        let system_metrics = SystemMetrics::new();
        system_metrics.register_metrics();
        tracing::info!("OpenTelemetry metrics initialized");
    }

    let address = format!("{}:{}", settings.host, settings.port);
    let listener = TcpListener::bind(&address)?;
    tracing::info!("Starting server on {}", address);

    run(listener, settings)?.await
}
