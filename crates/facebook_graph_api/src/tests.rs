// As to 28/12/2025, Facebook has disabled test users so they give me no choice but to Mock their
// API, using the Graph API explorer

use super::*;
use expect_test::{Expect, expect};
use reqwest::Client;
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, query_param},
};

fn check(status: ErrorCode, expect: Expect) {
    expect.assert_debug_eq(&status);
}

fn check_reason(canonical_reason: Option<&str>, expect: Expect) {
    expect.assert_debug_eq(&canonical_reason);
}

#[tokio::test]
async fn facebook_v24_0_me_190() {
    let mock_server = MockServer::start().await;

    let error_body = json!({
        "error": {
            "message": "Error validating access token: Session has expired.",
            "type": "OAuthException",
            "code": 190,
            "error_subcode": 463,
            "fbtrace_id": "Ab7Udf3hEvk7tW7RrcY8No9"
        }
    });

    Mock::given(method("GET"))
        .and(path("/me"))
        .and(query_param("fields", "id,name"))
        .respond_with(ResponseTemplate::new(400).set_body_json(&error_body))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = Client::new();

    let resp = client
        .get(format!("{}/me", mock_server.uri()))
        .query(&[("fields", "id,name")])
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);

    let body: serde_json::Value = resp.json().await.expect("invalid json");

    let err = &body["error"];

    let code = err["code"].as_u64().unwrap() as u32;
    let subcode = err["error_subcode"].as_u64().unwrap() as u16;

    let status = ErrorCode::from_parts(code, Some(subcode)).expect("invalid fb status code");

    assert_eq!(status, ErrorCode::TOKEN_EXPIRED);
    check(
        status,
        expect![[r#"
            ErrorCode(190, Some(463))
        "#]],
    );
    check_reason(
        status.canonical_reason(),
        expect![[r#"
            Some(
                "Login status or access token has expired, been revoked, or is invalid",
            )
        "#]],
    );
}
