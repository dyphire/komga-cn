use super::*;

#[tokio::test]
async fn books_route_returns_json_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_snapshot("books-list.json"));
}

#[tokio::test]
async fn books_latest_route_returns_json_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/latest?unpaged=true")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_snapshot("books-list.json"));
}

#[tokio::test]
async fn books_latest_route_uses_java_live_localdb_upstream_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body: Value = serde_json::from_str(
        r#"{
            "content": [
                {
                    "created": "2026-03-21T14:11:14Z",
                    "deleted": false,
                    "fileHash": "",
                    "fileLastModified": "2024-01-02T08:04:05Z",
                    "id": "book-1",
                    "lastModified": "2026-03-21T14:11:14Z",
                    "libraryId": "1",
                    "media": {
                        "comment": "",
                        "epubDivinaCompatible": false,
                        "epubIsKepub": false,
                        "mediaProfile": "DIVINA",
                        "mediaType": "application/zip",
                        "pagesCount": 1,
                        "status": "READY"
                    },
                    "metadata": {
                        "authors": [],
                        "authorsLock": false,
                        "created": "2026-03-21T14:11:14Z",
                        "isbn": "",
                        "isbnLock": false,
                        "lastModified": "2026-03-21T14:11:14Z",
                        "links": [],
                        "linksLock": false,
                        "number": "1",
                        "numberLock": false,
                        "numberSort": 1.0,
                        "numberSortLock": false,
                        "releaseDate": "2024-01-01",
                        "releaseDateLock": false,
                        "summary": "",
                        "summaryLock": false,
                        "tags": [],
                        "tagsLock": false,
                        "title": "book.cbr",
                        "titleLock": false
                    },
                    "name": "book.cbr",
                    "number": 1,
                    "oneshot": false,
                    "readProgress": null,
                    "seriesId": "series-1",
                    "seriesTitle": "series",
                    "size": "222 B",
                    "sizeBytes": 222,
                    "url": "/tmp/komga-live-http-fixture/library1/series/book.cbr"
                }
            ],
            "pageable": {
                "pageNumber": 0,
                "pageSize": 20,
                "sort": {
                    "empty": false,
                    "sorted": true,
                    "unsorted": false
                },
                "offset": 0,
                "paged": true,
                "unpaged": false
            },
            "last": true,
            "totalElements": 1,
            "totalPages": 1,
            "first": true,
            "size": 20,
            "number": 0,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "numberOfElements": 1,
            "empty": false
        }"#,
    )
    .unwrap();
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/books/latest?unpaged=true "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/latest?unpaged=true")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn snapshot_profile_uses_expected_series_and_book_urls() {
    let default_app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::SnapshotAligned);
    assert_series_and_books_urls(default_app, "", "book.cbr").await;
}

#[tokio::test]
async fn books_route_uses_java_live_localdb_upstream_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body: Value = serde_json::from_str(
        r#"{
            "content": [
                {
                    "created": "2026-03-21T13:54:33Z",
                    "deleted": false,
                    "fileHash": "",
                    "fileLastModified": "2024-01-02T08:04:05Z",
                    "id": "book-1",
                    "lastModified": "2026-03-21T13:54:33Z",
                    "libraryId": "1",
                    "media": {
                        "comment": "",
                        "epubDivinaCompatible": false,
                        "epubIsKepub": false,
                        "mediaProfile": "DIVINA",
                        "mediaType": "application/zip",
                        "pagesCount": 1,
                        "status": "READY"
                    },
                    "metadata": {
                        "authors": [],
                        "authorsLock": false,
                        "created": "2026-03-21T13:54:33Z",
                        "isbn": "",
                        "isbnLock": false,
                        "lastModified": "2026-03-21T13:54:33Z",
                        "links": [],
                        "linksLock": false,
                        "number": "1",
                        "numberLock": false,
                        "numberSort": 1.0,
                        "numberSortLock": false,
                        "releaseDate": "2024-01-01",
                        "releaseDateLock": false,
                        "summary": "",
                        "summaryLock": false,
                        "tags": [],
                        "tagsLock": false,
                        "title": "book.cbr",
                        "titleLock": false
                    },
                    "name": "book.cbr",
                    "number": 1,
                    "oneshot": false,
                    "readProgress": null,
                    "seriesId": "series-1",
                    "seriesTitle": "series",
                    "size": "222 B",
                    "sizeBytes": 222,
                    "url": "/tmp/komga-live-http-fixture/library1/series/book.cbr"
                }
            ],
            "pageable": {
                "pageNumber": 0,
                "pageSize": 20,
                "sort": {
                    "empty": true,
                    "sorted": false,
                    "unsorted": true
                },
                "offset": 0,
                "paged": true,
                "unpaged": false
            },
            "last": true,
            "totalElements": 1,
            "totalPages": 1,
            "first": true,
            "size": 20,
            "number": 0,
            "sort": {
                "empty": true,
                "sorted": false,
                "unsorted": true
            },
            "numberOfElements": 1,
            "empty": false
        }"#,
    )
    .unwrap();
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/books "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn book_pages_route_uses_java_live_localdb_page_metadata() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([
        {
            "number": 1,
            "fileName": "page-0001.png",
            "mediaType": "image/png",
            "width": 1,
            "height": 1,
            "sizeBytes": 69,
            "size": "69 B"
        }
    ]);
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/books/book-1/pages "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn book_read_progress_get_route_matches_java_live_method_not_allowed_contract() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!({
        "error": "Method Not Allowed",
        "message": "Method 'GET' is not supported.",
        "path": "/api/v1/books/book-1/read-progress",
        "status": 405,
        "timestamp": "2026-03-21T14:34:26.083+00:00",
        "trace": "org.springframework.web.HttpRequestMethodNotSupportedException: Request method 'GET' is not supported",
    });
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/books/book-1/read-progress "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 405 Method Not Allowed\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn book_read_progress_patch_route_accepts_completed_true() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"completed":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn book_read_progress_patch_route_accepts_page_one() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"page":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn book_read_progress_patch_route_rejects_invalid_page() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"page":999}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
}

#[tokio::test]
async fn book_read_progress_patch_route_returns_not_found_for_other_books() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-2/read-progress")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"completed":true}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn book_read_progress_delete_route_returns_no_content_for_existing_book() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn book_read_progress_delete_route_returns_not_found_for_missing_book() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/book-missing/read-progress")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn book_progression_patch_route_accepts_valid_progression_payload() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/progression")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{"progression":0.3}}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn book_progression_get_route_returns_no_content_when_no_progression_exists() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4="),
                    "unexpected bootstrap request: {request_lower}"
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/books/book-1/progression "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                concat!(
                    "HTTP/1.1 204 No Content\r\n",
                    "Content-Length: 0\r\n",
                    "Connection: close\r\n",
                    "\r\n"
                )
                .to_string()
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/progression")
                .header("X-Auth-Token", token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(response.headers().get(header::CONTENT_TYPE).is_none());
}

#[tokio::test]
async fn book_progression_patch_route_rejects_payload_without_progression() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/progression")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{}}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
}

#[tokio::test]
async fn book_read_progress_changes_are_isolated_per_auth_token() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!({
        "content": [
            {
                "id": "book-1",
                "readProgress": null,
            }
        ]
    });
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..4 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step % 2 == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic ywrtaw5azxhhbxbszs5vcmc6ywrtaw4=")
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-admin-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/books "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    upstream_response.len(),
                    upstream_response
                )
            };

            tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
                .await
                .unwrap();
        }
    });

    let env_key = "KOMGA_RUST_JAVA_LIVE_BASE_URL";
    let original = std::env::var_os(env_key);
    unsafe {
        std::env::set_var(env_key, format!("http://{address}"));
    }

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let patch_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", "token-user-1")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"page":1}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let user_one_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("X-Auth-Token", "token-user-1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(user_one_response.status(), StatusCode::OK);

    let user_one_body = axum::body::to_bytes(user_one_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let user_one_json: Value = serde_json::from_slice(&user_one_body).unwrap();
    assert_eq!(user_one_json["content"][0]["readProgress"]["page"], 1);

    let user_two_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("X-Auth-Token", "token-user-2")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(user_two_response.status(), StatusCode::OK);

    if let Some(value) = original {
        unsafe {
            std::env::set_var(env_key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(env_key);
        }
    }

    server.abort();
    let _ = server.await;

    let user_two_body = axum::body::to_bytes(user_two_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let user_two_json: Value = serde_json::from_slice(&user_two_body).unwrap();
    assert!(user_two_json["content"][0]["readProgress"].is_null());
}

#[tokio::test]
async fn book_pages_route_returns_a_valid_placeholder_list_in_snapshot_aligned_profile() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json[0]["number"], 1);
    assert_eq!(json[0]["fileName"], "komga.png");
    assert_eq!(json[0]["sizeBytes"], 0);
}

#[tokio::test]
async fn book_page_route_supports_pdf_and_cache_headers() {
    let app = komga_rust::app::build_router();

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1")
                .header(header::ACCEPT, "application/pdf")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/pdf"
    );
    let last_modified = response
        .headers()
        .get(header::LAST_MODIFIED)
        .unwrap()
        .clone();

    let cached = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1")
                .header(header::ACCEPT, "application/pdf")
                .header(header::IF_MODIFIED_SINCE, last_modified)
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        cached.headers().get(header::CACHE_CONTROL).unwrap(),
        "max-age=0, must-revalidate, private"
    );
}

#[tokio::test]
async fn book_page_route_uses_png_download_headers_in_java_live_localdb() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/png"
    );
    assert_eq!(
        response.headers().get(header::LAST_MODIFIED).unwrap(),
        "Mon, 01 Jan 2024 22:04:05 GMT"
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap(),
        "inline; filename=\"=?UTF-8?Q?book.cbr-1.png?=\"; filename*=UTF-8''book.cbr-1.png"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!body.is_empty());

    let cached = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1")
                .header("X-Auth-Token", "dummy-token")
                .header(header::IF_MODIFIED_SINCE, "Mon, 01 Jan 2024 22:04:05 GMT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        cached.headers().get(header::LAST_MODIFIED).unwrap(),
        "Mon, 01 Jan 2024 22:04:05 GMT"
    );
}

#[tokio::test]
async fn book_page_thumbnail_route_uses_jpeg_headers_in_java_live_localdb() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1/thumbnail")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg"
    );
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "max-age=0, must-revalidate, private"
    );
    assert_eq!(
        response.headers().get(header::LAST_MODIFIED).unwrap(),
        "Mon, 01 Jan 2024 22:04:05 GMT"
    );
    assert_eq!(
        response.headers().get(header::ETAG).unwrap(),
        "\"048bbf960d13687d84948688ab74aaa59\""
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!body.is_empty());

    let cached = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1/thumbnail")
                .header("X-Auth-Token", "dummy-token")
                .header(header::IF_MODIFIED_SINCE, "Mon, 01 Jan 2024 22:04:05 GMT")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        cached.headers().get(header::LAST_MODIFIED).unwrap(),
        "Mon, 01 Jan 2024 22:04:05 GMT"
    );
}

#[tokio::test]
async fn book_thumbnail_route_returns_not_found_in_java_live_localdb_seeded_fixture() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/thumbnail")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn book_thumbnail_route_returns_not_found_in_snapshot_profile() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/thumbnail")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(body.is_empty());
}

#[tokio::test]
async fn book_page_thumbnail_route_returns_placeholder_in_snapshot_profile() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1/thumbnail")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!body.is_empty());
}

#[tokio::test]
async fn book_file_route_returns_download_headers() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/file")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/pdf"
    );
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(disposition.starts_with("attachment;"));
    assert!(disposition.contains("filename*=UTF-8''"));
}

#[tokio::test]
async fn book_file_route_uses_zip_download_headers_in_java_live_localdb() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/file")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/zip"
    );
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .unwrap()
            .to_str()
            .unwrap(),
        "attachment; filename=\"=?UTF-8?Q?book.cbr?=\"; filename*=UTF-8''book.cbr"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert!(!body.is_empty());
}
