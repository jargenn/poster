use crate::helpers::{check, spawn_app};
use expect_test::expect;
use http::StatusCode;
use pretty_assertions::assert_eq;
use serde_json::Value;

#[tokio::test]
async fn valid_config_is_accepted() {
    let app = spawn_app().await;
    app.seed_logging().await;

    let body = serde_json::json!({
        "id": "1408225520952821",
        "secret": "124d12412d1",
        "config_id": "25537253695887249",
        "redirect_url": "http://localhost:3000/facebook/oauth/callback",
        "description": "Poster en TNEA"
    });

    let response = app.post_config(&body).await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let body = response
        .json::<Value>()
        .await
        .expect("Failed to parse response as Value");

    let message = body
        .get("message:")
        .and_then(|v| v.as_str())
        .expect("message: field not found or not a string");

    check(
        message,
        expect![[r#"
            "Config saved!"
        "#]],
    );
}

#[tokio::test]
async fn invalid_configs_are_rejected() {
    let app = spawn_app().await;
    app.seed_logging().await;

    // TODO: Proptest this
    let invalid_bodies = vec![
        serde_json::json!({
            "id": "1408225520952821",
        }),
        serde_json::json!({
            "secret": "124d12412d1",
        }),
        serde_json::json!({
            "config_id": "25537253695887249",
        }),
        serde_json::json!({
            "redirect_url": "http://localhost:3000/facebook/oauth/callback",
        }),
        serde_json::json!({
            "description": "Poster en TNEA"
        }),
    ];

    for body in invalid_bodies {
        let response = app.post_config(&body).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
