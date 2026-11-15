// Only enables OpenSSL's `vendored` feature for the static musl build.
use openssl as _;

pub mod app_settings;
pub mod auth;
pub mod http_span;
pub mod routes;
pub mod startup;
pub mod storage;
pub mod telemetry;
