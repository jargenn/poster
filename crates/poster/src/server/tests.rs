// // As to 28/12/2025, Facebook has disabled test users so they give me no choice but to Mock their
// // API, using the Graph API explorer

// use super::*;
// use serde_json::json;
// // use expect_test::{Expect, expect};
// use wiremock::{
//     Mock, MockServer, ResponseTemplate,
//     matchers::{method, path, query_param},
// };

// // fn check(query: &str, expect: Expect) {
// //     let res = todo!();
// //     expect.assert_debug_eq(&res);
// // }

// #[tokio::test]
// async fn facebook_v24_0_me_200() {
//     let mock_server = MockServer::start().await;

//     let response_body = json!({
//         "id": "3863045207321253",
//         "name": "Test User"
//     });

//     Mock::given(method("GET"))
//         .and(path("/me"))
//         .and(query_param("fields", "id,name"))
//         .respond_with(ResponseTemplate::new(200).set_body_json(response_body))
//         .expect(1)
//         .mount(&mock_server)
//         .await;

//     // Call your client pointing to `mock_server.uri()` instead of the real Graph API
// }

// #[tokio::test]
// async fn facebook_v24_0_me_190() {
//     let mock_server = MockServer::start().await;

//     let error_body = json!({
//         "error": {
//             "message": "Error validating access token: Session has expired.",
//             "type": "OAuthException",
//             "code": 190,
//             "error_subcode": 463,
//             "fbtrace_id": "Ab7Udf3hEvk7tW7RrcY8No9"
//         }
//     });

//     Mock::given(method("GET"))
//         .and(path("/me"))
//         .and(query_param("fields", "id,name"))
//         .respond_with(ResponseTemplate::new(400).set_body_json(error_body))
//         .expect(1)
//         .mount(&mock_server)
//         .await;

//     // Call your client pointing to `mock_server.uri()`
//     // Assert that your code handles token expiration correctly
// }
