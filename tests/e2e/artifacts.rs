use crate::helpers::{READ_ONLY_TOKEN, READ_WRITE_TOKEN, spawn_app};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test]
async fn get_artifact_cache_miss_returns_404() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // Mock S3 to return 404 (NoSuchKey)
    Mock::given(method("GET"))
        .and(path("/test-bucket/rush-cache/nonexistent-id"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&app.mock_s3)
        .await;

    let response = client
        .get(format!("{}/artifacts/nonexistent-id", app.address))
        .header("Authorization", format!("Bearer {}", READ_ONLY_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn get_artifact_cache_hit_returns_200_with_body() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let artifact_data = b"fake tar gz content here";

    // Mock S3 to return the artifact
    Mock::given(method("GET"))
        .and(path("/test-bucket/rush-cache/hit-cache-id"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(artifact_data.to_vec()))
        .mount(&app.mock_s3)
        .await;

    let response = client
        .get(format!("{}/artifacts/hit-cache-id", app.address))
        .header("Authorization", format!("Bearer {}", READ_ONLY_TOKEN))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);
    let body = response.bytes().await.unwrap();
    assert_eq!(body.as_ref(), artifact_data);
}

#[tokio::test]
async fn put_artifact_stores_to_s3_and_returns_200() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let artifact_data = b"build output tar gz content";

    // Mock S3 to accept the PUT
    Mock::given(method("PUT"))
        .and(path("/test-bucket/rush-cache/new-cache-id"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&app.mock_s3)
        .await;

    let response = client
        .put(format!("{}/artifacts/new-cache-id", app.address))
        .header("Authorization", format!("Bearer {}", READ_WRITE_TOKEN))
        .body(artifact_data.to_vec())
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["success"], true);
}

#[tokio::test]
async fn put_then_get_roundtrip() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let artifact_data = b"roundtrip test data 1234567890";
    let cache_id = "roundtrip-cache-id";

    // Mock S3 PUT
    Mock::given(method("PUT"))
        .and(path(format!("/test-bucket/rush-cache/{}", cache_id)))
        .respond_with(ResponseTemplate::new(200))
        .mount(&app.mock_s3)
        .await;

    // Mock S3 GET (returns the same data)
    Mock::given(method("GET"))
        .and(path(format!("/test-bucket/rush-cache/{}", cache_id)))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(artifact_data.to_vec()))
        .mount(&app.mock_s3)
        .await;

    // PUT
    let put_response = client
        .put(format!("{}/artifacts/{}", app.address, cache_id))
        .header("Authorization", format!("Bearer {}", READ_WRITE_TOKEN))
        .body(artifact_data.to_vec())
        .send()
        .await
        .expect("Failed to PUT");
    assert_eq!(put_response.status(), 200);

    // GET
    let get_response = client
        .get(format!("{}/artifacts/{}", app.address, cache_id))
        .header("Authorization", format!("Bearer {}", READ_WRITE_TOKEN))
        .send()
        .await
        .expect("Failed to GET");
    assert_eq!(get_response.status(), 200);
    let body = get_response.bytes().await.unwrap();
    assert_eq!(body.as_ref(), artifact_data);
}

#[tokio::test]
async fn put_without_auth_returns_401() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .put(format!("{}/artifacts/some-cache-id", app.address))
        .body(b"data".to_vec())
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);
}

#[tokio::test]
async fn get_without_auth_returns_401() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/artifacts/some-cache-id", app.address))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 401);
}
