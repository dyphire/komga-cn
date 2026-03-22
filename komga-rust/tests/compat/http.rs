use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::util::ServiceExt;

pub async fn session_token_for_basic_auth<S>(app: &S, basic_auth: &str) -> String
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_auth}"))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .expect("login response should include x-auth-token")
        .to_str()
        .expect("x-auth-token should be valid UTF-8")
        .to_string()
}

pub async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

pub fn page_content_ids(value: &Value) -> Vec<&str> {
    value["content"]
        .as_array()
        .expect("page payload should expose array content")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("page item id should be a string")
        })
        .collect()
}
