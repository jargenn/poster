use crate::helpers::{check, spawn_app};
use expect_test::expect;
use http::{HeaderMap, StatusCode};
use pretty_assertions::assert_eq;
use uuid::Uuid;
use wiremock::{
    Mock, ResponseTemplate,
    matchers::{method, path},
};

#[tokio::test]
async fn scheduling_post_is_indempotent() {
    let app = spawn_app().await;
    app.seed_logging().await;

    // I test this endpoint because it is always called before trying to post something, because
    // we need the page_access_token it returns. So by testing the times I hit this, I can
    // understand how many times I would have hit the correct endpoint
    Mock::given(path("/v24.0/test_user_id/accounts"))
        .and(method("GET"))
        .respond_with(ResponseTemplate::new(202))
        .expect(1)
        .mount(&app.facebook_server)
        .await;

    let body = serde_json::json!([
        {
            "content": "Test app 1",
            "media": [
                "https://pub-4f1f80e5179d47208b7c305470c72eac.r2.dev/Screenshot_2025-10-22_00-32-31.png"
            ],
            // "idempotency_key": uuid::Uuid::new_v4().to_string()
        }
    ]);

    let mut headers = HeaderMap::new();

    headers.insert(
        "Idempotency-Key",
        Uuid::new_v4().to_string().parse().unwrap(),
    );

    app.submit_post(&headers, &body).await;
    // FIX: Test this case
    app.submit_post(&headers, &body).await;
}

#[tokio::test]
async fn scheduling_post_fails_fast_on_missing_image_format() {
    let app = spawn_app().await;
    app.seed_logging().await;

    let body = serde_json::json!([
        {
            "content": "Test app 1",
            "media": [
                "https://pub-4f1f80e5179d47208b7c305470c72eac.r2.dev/Screenshot_2025-10-22_00-32-31"
            ],
            // "idempotency_key": uuid::Uuid::new_v4().to_string()
        }
    ]);

    let mut headers = HeaderMap::new();

    headers.insert(
        "Idempotency-Key",
        Uuid::new_v4().to_string().parse().unwrap(),
    );

    let response = app.submit_post(&headers, &body).await;

    let body = response
        .text()
        .await
        .expect("Failed to read the response body");

    check(
        body,
        expect![[r#"
        "{\"error\":\"Image validation failed: Couldn't define the image format\"}"
    "#]],
    );
}

#[tokio::test]
async fn scheduling_post_fails_fails_on_missing_content() {
    let app = spawn_app().await;

    let body = serde_json::json!([
        {
            "media": [
                "https://pub-4f1f80e5179d47208b7c305470c72eac.r2.dev/Screenshot_2025-10-22_00-32-31"
            ],
            // "idempotency_key": uuid::Uuid::new_v4().to_string()
        }
    ]);

    let mut headers = HeaderMap::new();

    headers.insert(
        "Idempotency-Key",
        Uuid::new_v4().to_string().parse().unwrap(),
    );

    let response = app.submit_post(&headers, &body).await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);

    let body = response
        .text()
        .await
        .expect("Failed to read the response body");

    check(
        body,
        expect![[r#"
            "Failed to deserialize the JSON body into the target type: [0]: missing field `content` at line 1 column 97"
        "#]],
    );
}
