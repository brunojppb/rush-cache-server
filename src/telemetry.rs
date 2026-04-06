use opentelemetry::trace::TracerProvider;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::trace::SdkTracerProvider;
use std::sync::{Arc, Mutex};
use sysinfo::System;
use tracing::subscriber::set_global_default;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

/// Check if OpenTelemetry is disabled via OTEL_SDK_DISABLED env var.
pub fn is_otel_disabled() -> bool {
    std::env::var("OTEL_SDK_DISABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false)
}

/// Initialize the tracing subscriber with optional OpenTelemetry and file logging.
pub fn init_telemetry(log_level: &str, logs_directory: Option<&str>) {
    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(log_level));

    let formatting_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_target(true)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true);

    if is_otel_disabled() {
        let subscriber = Registry::default().with(env_filter).with(formatting_layer);
        set_global_default(subscriber).expect("Failed to set tracing subscriber");
    } else {
        let tracer_provider = init_tracer_provider();
        let tracer = tracer_provider.tracer("rush-cache-server");
        let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

        let subscriber = Registry::default()
            .with(env_filter)
            .with(formatting_layer)
            .with(otel_layer);
        set_global_default(subscriber).expect("Failed to set tracing subscriber");
    }

    if let Some(dir) = logs_directory {
        let file_appender = tracing_appender::rolling::daily(dir, "rush-cache-server.log");
        let _guard = tracing_appender::non_blocking(file_appender);
        // Note: _guard must be held for the lifetime of the application.
        // In production, this is stored in main().
    }
}

fn init_tracer_provider() -> SdkTracerProvider {
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(
            std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
        )
        .build()
        .expect("Failed to create OTLP span exporter");

    SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .build()
}

/// Initialize the OpenTelemetry meter provider for metrics.
pub fn init_meter_provider() -> Option<SdkMeterProvider> {
    if is_otel_disabled() {
        return None;
    }

    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(
            std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
        )
        .build()
        .expect("Failed to create OTLP metric exporter");

    let provider = SdkMeterProvider::builder()
        .with_periodic_exporter(exporter)
        .build();

    opentelemetry::global::set_meter_provider(provider.clone());
    Some(provider)
}

/// System metrics collector using sysinfo.
pub struct SystemMetrics {
    system: Arc<Mutex<System>>,
}

impl Default for SystemMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            system: Arc::new(Mutex::new(System::new_all())),
        }
    }

    /// Register observable gauges for CPU and memory with the global meter.
    pub fn register_metrics(&self) {
        let meter = opentelemetry::global::meter("rush-cache-server");
        let system = self.system.clone();

        let _cpu_gauge = meter
            .f64_observable_gauge("system.cpu.utilization")
            .with_description("Process CPU utilization")
            .with_callback({
                let system = system.clone();
                move |observer| {
                    if let Ok(mut sys) = system.lock() {
                        sys.refresh_cpu_all();
                        let usage = sys.global_cpu_usage() as f64;
                        observer.observe(usage, &[]);
                    }
                }
            })
            .build();

        let _mem_gauge = meter
            .u64_observable_gauge("system.memory.usage")
            .with_description("Process memory usage in bytes")
            .with_callback({
                let system = system.clone();
                move |observer| {
                    if let Ok(mut sys) = system.lock() {
                        sys.refresh_memory();
                        observer.observe(sys.used_memory(), &[]);
                    }
                }
            })
            .build();
    }
}
