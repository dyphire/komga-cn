use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::util::ServiceExt;

mod compat;

const NATIVE_OWNERSHIP_HEADER: &str = "X-Komga-Compat-Search-Ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";

use compat::http::session_token_for_basic_auth;

#[path = "catalog_discovery_shadow/books.rs"]
mod books;
#[path = "catalog_discovery_shadow/libraries.rs"]
mod libraries;
#[path = "catalog_discovery_shadow/ownership.rs"]
mod ownership;
#[path = "catalog_discovery_shadow/parity.rs"]
mod parity;
#[path = "catalog_discovery_shadow/series.rs"]
mod series;

fn ids(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("libraries payload should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("library id should be a string")
        })
        .collect()
}

fn book_ids(value: &Value) -> Vec<&str> {
    value
        .get("content")
        .and_then(Value::as_array)
        .expect("books payload content should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("book id should be a string")
        })
        .collect()
}

fn page_content_ids(value: &Value) -> Vec<&str> {
    value
        .get("content")
        .and_then(Value::as_array)
        .expect("page payload content should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("content id should be a string")
        })
        .collect()
}

async fn libraries_json_for_token<S>(app: &S, token: &str) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", token)
                .header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn series_list_json_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = series_list_response_for_token(app, token, path, body, native_owned).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn series_list_response_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("X-Auth-Token", token)
        .header(header::CONTENT_TYPE, "application/json");

    if native_owned {
        request = request.header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
    }

    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn books_list_json_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = books_list_response_for_token(app, token, path, body, native_owned).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn books_list_response_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("X-Auth-Token", token)
        .header(header::CONTENT_TYPE, "application/json");

    if native_owned {
        request = request.header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
    }

    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn books_latest_json_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    native_owned: bool,
) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = books_latest_response_for_token(app, token, path, native_owned).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn books_latest_response_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    native_owned: bool,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder().uri(path).header("X-Auth-Token", token);

    if native_owned {
        request = request.header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
    }

    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
}
