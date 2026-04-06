use rush_cache_server::app_settings::{AppSettings, TokenStore};
use rush_cache_server::startup::run;
use std::collections::HashSet;
use std::net::TcpListener;
use std::sync::LazyLock;
use wiremock::MockServer;

pub static TRACING: LazyLock<()> = LazyLock::new(|| {
    if std::env::var("TEST_LOG").is_ok() {
        rush_cache_server::telemetry::init_telemetry("debug", None);
    }
});

pub const READ_ONLY_TOKEN: &str = "test_read_only_token";
pub const READ_WRITE_TOKEN: &str = "test_read_write_token";

pub struct TestApp {
    pub address: String,
    pub mock_s3: MockServer,
}

pub struct TestAppConfig {
    pub s3_prefix: String,
}

impl Default for TestAppConfig {
    fn default() -> Self {
        Self {
            s3_prefix: "rush-cache".to_string(),
        }
    }
}

pub async fn spawn_app() -> TestApp {
    spawn_app_with_config(TestAppConfig::default()).await
}

pub async fn spawn_app_with_config(config: TestAppConfig) -> TestApp {
    LazyLock::force(&TRACING);

    let mock_s3 = MockServer::start().await;

    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind random port");
    let port = listener.local_addr().unwrap().port();
    let address = format!("http://127.0.0.1:{}", port);

    let settings = AppSettings {
        host: "127.0.0.1".to_string(),
        port,
        s3_region: "us-east-1".to_string(),
        s3_bucket: "test-bucket".to_string(),
        s3_prefix: config.s3_prefix,
        s3_endpoint: Some(mock_s3.uri()),
        s3_access_key: Some("test-access-key".to_string()),
        s3_secret_key: Some("test-secret-key".to_string()),
        s3_use_path_style: true,
        max_body_size: 524_288_000,
        log_level: "info".to_string(),
        logs_directory: None,
        token_store: TokenStore::new(
            HashSet::from([READ_ONLY_TOKEN.to_string()]),
            HashSet::from([READ_WRITE_TOKEN.to_string()]),
        ),
    };

    let server = run(listener, settings).expect("Failed to start server");
    tokio::spawn(server);

    TestApp { address, mock_s3 }
}
