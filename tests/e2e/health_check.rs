use crate::helpers::spawn_app;

#[tokio::test]
async fn health_check_returns_200() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{}/health", app.address))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["status"], "healthy");
}

#[tokio::test]
async fn health_check_requires_no_auth() {
    let app = spawn_app().await;
    let client = reqwest::Client::new();

    // No Authorization header
    let response = client
        .get(format!("{}/health", app.address))
        .send()
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), 200);
}
