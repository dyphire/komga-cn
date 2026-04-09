use axum::body::Body;
use axum::body::to_bytes;
use axum::http::{Request, StatusCode, header};
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "../crates/interfaces/src/http/access_log.rs"]
mod access_log_impl;

async fn update_book_search_fixture_title(paths: &RuntimeDbPaths, book_id: &str, title: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("search webui db should open for title update");

    sqlx::query(
        "UPDATE BOOK_METADATA \
         SET TITLE = ? \
         WHERE BOOK_ID = ?",
    )
    .bind(title)
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("search webui title should update");

    pool.close().await;
}

fn single_http_access_event(logs: &str) -> serde_json::Map<String, Value> {
    let events = parse_json_log_lines(logs);
    let access_events = matching_event_fields(&events, "http_access");

    assert_eq!(
        access_events.len(),
        1,
        "expected exactly one http_access event: {logs}"
    );

    access_events[0].clone()
}

fn assert_get_access_event_basics(
    fields: &serde_json::Map<String, Value>,
    route: &str,
    path: &str,
    status_code: u64,
    outcome: &str,
) {
    assert_eq!(field_str(fields, "method"), Some("GET"));
    assert_eq!(field_str(fields, "route"), Some(route));
    assert_eq!(field_str(fields, "path"), Some(path));
    assert_eq!(field_u64(fields, "status_code"), Some(status_code));
    assert_eq!(field_str(fields, "outcome"), Some(outcome));
}

fn assert_queryless_timed_access_event(
    fields: &serde_json::Map<String, Value>,
    route: &str,
    path: &str,
    status_code: u64,
    outcome: &str,
) {
    assert_get_access_event_basics(fields, route, path, status_code, outcome);
    assert!(
        field_u64(fields, "latency_ms").is_some(),
        "expected latency_ms on access event: {fields:?}"
    );
    assert!(
        field_str(fields, "path").is_some_and(|value| !value.contains('?')),
        "path must exclude query strings: {fields:?}"
    );
}

fn assert_logs_omit_values(logs: &str, values: &[&str], context: &str) {
    for value in values {
        assert!(
            !logs.contains(value),
            "{context} must redact sensitive material `{value}`: {logs}"
        );
    }
}

#[test]
fn router_access_log_enriches_authenticated_user_and_redacts_sensitive_inputs() {
    let paths = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("search webui access log test runtime should build")
        .block_on(async {
            let paths = new_router_fixture("router-access-log-queryless-path-contract").await;
            seed_router_contract_data(&paths).await;
            seed_router_pdf_book(
                &paths,
                "book-pdf-access-log",
                "series-1",
                "access-log.pdf",
                "Access Log PDF",
            )
            .await;
            paths
        });
    let config = runtime_config_for_paths(&paths);
    let auth_token = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("search webui access log auth runtime should build")
        .block_on(async {
            let app = build_router_with_config(&config);
            login_with_basic_and_get_token(app).await
        });
    let (logs, response) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let auth_token = auth_token.clone();
        async move {
            let app = build_router_with_config(&config);

            let request_body_marker = "super-secret-body-marker";
            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v2/users/me?client=webui&downloadToken=secret-value")
                        .header("x-auth-token", &auth_token)
                        .header(header::AUTHORIZATION, "Bearer auth-header-secret")
                        .header(
                            header::COOKIE,
                            "komga-remember-me=remember-secret; KOMGA-SESSION=session-secret",
                        )
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from(request_body_marker))
                        .expect("access log route/path request should build"),
                )
                .await
                .expect("access log route/path request should complete");

            let (parts, body) = response.into_parts();
            let body_bytes = to_bytes(body, usize::MAX)
                .await
                .expect("route/path response body should be readable");
            axum::response::Response::from_parts(parts, Body::from(body_bytes))
        }
    });

    assert_eq!(response.status(), StatusCode::OK);
    let fields = single_http_access_event(&logs);
    assert_eq!(field_str(&fields, "method"), Some("GET"));
    assert_eq!(field_str(&fields, "route"), Some("/api/v2/users/me"));
    assert_eq!(field_str(&fields, "path"), Some("/api/v2/users/me"));
    assert_eq!(field_u64(&fields, "status_code"), Some(200));
    assert_eq!(field_str(&fields, "outcome"), Some("success"));
    assert_eq!(field_str(&fields, "user_id"), Some("admin-user"));
    assert!(
        field_str(&fields, "request_id").is_some_and(|value| !value.is_empty()),
        "expected http_access event to emit a non-empty request_id: {fields:?}"
    );
    assert!(
        field_u64(&fields, "latency_ms").is_some(),
        "expected latency_ms on access event: {fields:?}"
    );
    assert!(
        field_str(&fields, "path").is_some_and(|path| !path.contains('?')),
        "path must exclude query strings: {fields:?}"
    );
    assert!(
        fields.get("first_byte_ms").is_none(),
        "normal JSON/API requests must not log first_byte_ms: {fields:?}"
    );
    assert_logs_omit_values(
        &logs,
        &[
            auth_token.as_str(),
            "Authorization",
            "auth-header-secret",
            "x-auth-token",
            "Cookie",
            "remember-secret",
            "session-secret",
            "super-secret-body-marker",
            "downloadToken=secret-value",
            "admin@example.org",
        ],
        "access logs",
    );

    cleanup_router_fixture(paths);
}

#[test]
fn router_access_log_marks_deferred_stream_errors_as_server_error() {
    use std::pin::Pin;
    use std::task::{Context, Poll};

    use axum::Router;
    use axum::body::Bytes;
    use axum::http::Method;
    use axum::routing::get;
    use http_body::Frame;
    use tower_http::trace::TraceLayer;

    struct FailingBody {
        emitted_chunk: bool,
    }

    impl http_body::Body for FailingBody {
        type Data = Bytes;
        type Error = axum::Error;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            if !self.emitted_chunk {
                self.emitted_chunk = true;
                return Poll::Ready(Some(Ok(Frame::data(Bytes::from_static(b"partial")))));
            }

            Poll::Ready(Some(Err(axum::Error::new(std::io::Error::other(
                "stream exploded after first byte",
            )))))
        }
    }

    let paths = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("deferred stream access log test runtime should build")
        .block_on(async { new_router_fixture("router-access-log-deferred-error-contract").await });
    let config = runtime_config_for_paths(&paths);

    let (logs, read_result) = capture_router_logs_async_result(&config, async move {
        let app = Router::new()
            .route(
                "/api/v1/books/{book_id}/file",
                get(|| async {
                    axum::response::Response::builder()
                        .status(StatusCode::OK)
                        .body(Body::new(FailingBody {
                            emitted_chunk: false,
                        }))
                        .expect("failing body response should build")
                }),
            )
            .route_layer(axum::middleware::from_fn(
                access_log_impl::prepare_access_log_middleware,
            ))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(access_log_impl::make_request_span)
                    .on_request(access_log_impl::on_request)
                    .on_response(access_log_impl::on_response)
                    .on_failure(access_log_impl::on_failure),
            );

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/api/v1/books/book-1/file")
                    .body(Body::empty())
                    .expect("deferred stream request should build"),
            )
            .await
            .expect("deferred stream request should complete");

        to_bytes(response.into_body(), usize::MAX).await
    });

    let read_error = read_result.expect_err("deferred stream body should fail while reading");
    assert!(
        read_error
            .to_string()
            .contains("stream exploded after first byte"),
        "expected deferred stream read error context: {read_error}"
    );

    let fields = single_http_access_event(&logs);
    assert_eq!(field_u64(&fields, "status_code"), Some(200));
    assert_eq!(field_str(&fields, "outcome"), Some("server_error"));
    assert!(
        field_u64(&fields, "first_byte_ms").is_some(),
        "expected deferred stream error to keep first_byte_ms when first data already flushed: {fields:?}"
    );

    cleanup_router_fixture(paths);
}

#[test]
fn router_access_log_records_anonymous_user_for_unauthorized_request() {
    let paths = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("search webui anonymous access log test runtime should build")
        .block_on(async {
            let paths = new_router_fixture("router-access-log-anonymous-user-contract").await;
            seed_router_contract_data(&paths).await;
            paths
        });
    let config = runtime_config_for_paths(&paths);
    let (logs, response) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        async move {
            let app = build_router_with_config(&config);

            app.oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v2/users/me?downloadToken=should-not-log")
                    .header(header::AUTHORIZATION, "Bearer anonymous-secret")
                    .header(
                        header::COOKIE,
                        "komga-remember-me=anon-remember-secret; KOMGA-SESSION=anon-session-secret",
                    )
                    .body(Body::empty())
                    .expect("anonymous access log request should build"),
            )
            .await
            .expect("anonymous access log request should complete")
        }
    });

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let fields = single_http_access_event(&logs);
    assert_queryless_timed_access_event(
        &fields,
        "/api/v2/users/me",
        "/api/v2/users/me",
        401,
        "client_error",
    );
    assert_eq!(field_str(&fields, "user_id"), Some("anonymous"));
    assert_logs_omit_values(
        &logs,
        &[
            "Authorization",
            "anonymous-secret",
            "Cookie",
            "anon-remember-secret",
            "anon-session-secret",
            "downloadToken=should-not-log",
        ],
        "anonymous access logs",
    );

    cleanup_router_fixture(paths);
}

#[test]
fn router_access_log_records_download_first_byte_without_chunk_spam() {
    let paths = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("search webui download access log test runtime should build")
        .block_on(async {
            let paths = new_router_fixture("router-access-log-download-first-byte-contract").await;
            seed_router_contract_data(&paths).await;
            seed_router_pdf_book(
                &paths,
                "book-pdf-first-byte",
                "series-1",
                "first-byte.pdf",
                "First Byte PDF",
            )
            .await;
            paths
        });
    let config = runtime_config_for_paths(&paths);
    let auth_token = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("search webui download access log auth runtime should build")
        .block_on(async {
            let app = build_router_with_config(&config);
            login_with_basic_and_get_token(app).await
        });
    let (logs, response) = capture_router_logs_async_result(&config, {
        let config = config.clone();
        let auth_token = auth_token.clone();
        async move {
            let app = build_router_with_config(&config);

            let response = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/books/book-pdf-first-byte/file?client=webui")
                        .header("x-auth-token", &auth_token)
                        .body(Body::empty())
                        .expect("download access log request should build"),
                )
                .await
                .expect("download access log request should complete");

            let (parts, body) = response.into_parts();
            let body_bytes = to_bytes(body, usize::MAX)
                .await
                .expect("download response body should be readable");
            (
                axum::response::Response::from_parts(parts, Body::from(body_bytes.clone())),
                body_bytes,
            )
        }
    });

    let (response, body_bytes) = response;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        !body_bytes.is_empty(),
        "download response should include bytes"
    );

    let fields = single_http_access_event(&logs);
    assert_get_access_event_basics(
        &fields,
        "/api/v1/books/{book_id}/file",
        "/api/v1/books/book-pdf-first-byte/file",
        200,
        "success",
    );

    let first_byte_ms = field_u64(&fields, "first_byte_ms")
        .expect("download access event should record first_byte_ms");
    let latency_ms =
        field_u64(&fields, "latency_ms").expect("download access event should record latency_ms");

    assert!(
        first_byte_ms <= latency_ms,
        "first byte should not exceed total latency: {fields:?}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_locks_webui_retained_search_parity() {
    let paths = new_router_fixture("router-discovery-books-list-webui-main-search-parity").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matrix = [
        (
            "plain relevance",
            Some("book"),
            Some("relevance,desc"),
            vec!["book-1", "book-2", "book-3"],
        ),
        (
            "fielded relevance",
            Some("title:book"),
            Some("relevance,desc"),
            vec!["book-1", "book-2", "book-3"],
        ),
        (
            "blank search",
            Some("   "),
            Some("metadata.title,asc"),
            vec!["book-1", "book-2", "book-3"],
        ),
        (
            "invalid query",
            Some("title:("),
            Some("relevance,desc"),
            vec![],
        ),
    ];

    for (label, full_text_search, sort, expected_ids) in matrix {
        let mut uri = String::from("/api/v1/books/list?page=0&size=20");
        if let Some(sort) = sort {
            uri.push_str("&sort=");
            uri.push_str(sort);
        }

        let mut body = json!({
            "condition": {
                "seriesId": {
                    "operator": "is",
                    "value": "series-1"
                }
            }
        });
        if let Some(full_text_search) = full_text_search {
            body["fullTextSearch"] = Value::String(full_text_search.to_string());
            body["condition"] = json!({
                "type": "Title",
                "operator": "contains",
                "value": "book"
            });
        }

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("books/list webui parity request should build"),
            )
            .await
            .expect("books/list webui parity request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("books/list webui parity payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids, "case: {label}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_webui_retains_accent_folded_and_cjk_recall() {
    let paths = new_router_fixture("router-discovery-books-list-webui-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    update_book_search_fixture_title(&paths, "book-1", "Café 東京 Book 1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=relevance,desc")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "fullTextSearch": "cafe 東京",
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        }
                    })
                    .to_string(),
                ))
                .expect("books/list webui accent+cjk request should build"),
        )
        .await
        .expect("books/list webui accent+cjk request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let ids = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list webui accent+cjk payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["book-1"],
        "books/list webui requests should retain accent-folded mixed CJK recall",
    );

    cleanup_router_fixture(paths);
}
