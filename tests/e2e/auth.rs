use crate::helpers::{READ_ONLY_TOKEN, READ_WRITE_TOKEN, spawn_app};

#[tokio::test]
async fn missing_auth_header_returns_401() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/artifacts/test-cache-id", app.address))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "Missing or malformed Authorization header");
}

#[tokio::test]
async fn invalid_token_returns_401() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/artifacts/test-cache-id", app.address))
        .header("Authorization", "Bearer invalid_token_here")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "Invalid token");
}

#[tokio::test]
async fn malformed_auth_header_returns_401() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // No "Bearer " prefix
    let response = client
        .get(format!("{}/artifacts/test-cache-id", app.address))
        .header("Authorization", "Basic some_credentials")
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn read_only_token_on_put_returns_403() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .put(format!("{}/artifacts/test-cache-id", app.address))
        .header("Authorization", format!("Bearer {}", READ_ONLY_TOKEN))
        .body(b"some data".to_vec())
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 403);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["error"], "Token does not have write permission");
}

#[tokio::test]
async fn read_write_token_on_get_is_allowed() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // This will hit the mock S3 and likely 404, but the point is it doesn't 401/403
    let response = client
        .get(format!("{}/artifacts/test-cache-id", app.address))
        .header("Authorization", format!("Bearer {}", READ_WRITE_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    // Should not be 401 or 403
    assert_ne!(response.status(), 401);
    assert_ne!(response.status(), 403);
}

#[tokio::test]
async fn read_only_token_on_get_is_allowed() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/artifacts/test-cache-id", app.address))
        .header("Authorization", format!("Bearer {}", READ_ONLY_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    // Should not be 401 or 403
    assert_ne!(response.status(), 401);
    assert_ne!(response.status(), 403);
}
