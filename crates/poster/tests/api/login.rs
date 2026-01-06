use expect_test::expect;

use crate::helpers::{check, spawn_app};

#[tokio::test]
async fn login_fails_with_invalid_form_data() {
    let app = spawn_app().await;

    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password"
    });

    let response = app.post_login(&login_body).await;

    check(
        response.text().await.expect("Failed to get response body"),
        expect![[r#"
            "{\"error\":\"The user credentials given are invalid. Please check if you wrote them correctly.\"}"
        "#]],
    );
}

#[tokio::test]
async fn login_is_successful_with_seeded_data() {
    let app = spawn_app().await;
    app.seed_logging().await;
}
