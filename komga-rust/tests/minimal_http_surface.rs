use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::LazyLock;
use tower::ServiceExt;

static JAVA_LIVE_BASE_URL_ENV_LOCK: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tokio::test]
async fn protected_routes_reject_missing_or_empty_auth_token() {
    let app = komga_rust::app::build_router();

    for (method, uri) in [
        ("GET", "/api/v1/libraries"),
        ("GET", "/api/v1/series"),
        ("GET", "/api/v1/books"),
        ("GET", "/api/v1/books/book-1/pages"),
        ("GET", "/api/v1/books/book-1/pages/1"),
        ("GET", "/api/v1/books/book-1/pages/1/thumbnail"),
        ("GET", "/api/v1/books/book-1/thumbnail"),
        ("GET", "/api/v1/books/book-1/file"),
        ("PATCH", "/api/v1/books/book-1/read-progress"),
        ("DELETE", "/api/v1/books/book-1/read-progress"),
        ("PATCH", "/api/v1/books/book-1/progression"),
        ("GET", "/opds/v1.2/series"),
        ("GET", "/opds/v2/books/book-1/manifest"),
        ("GET", "/api/v1/login/set-cookie"),
    ] {
        let missing = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            missing.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} should reject missing token"
        );

        let empty = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header("X-Auth-Token", "")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            empty.status(),
            StatusCode::UNAUTHORIZED,
            "{uri} should reject empty token"
        );
    }
}

#[tokio::test]
async fn libraries_route_returns_json_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
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
    assert_eq!(json, expected_snapshot("libraries-list-admin.json"));
}

#[tokio::test]
async fn opds_v1_series_route_matches_java_live_localdb_seeded_entry_contract() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v1.2/series")
                .header("X-Auth-Token", "dummy-token")
                .header(header::HOST, "komga.local")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/atom+xml"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();

    assert_eq!(
        body,
        concat!(
            "<feed xmlns=\"http://www.w3.org/2005/Atom\">",
            "<id>allSeries</id>",
            "<title>All series</title>",
            "<updated>2026-01-01T00:00:00Z</updated>",
            "<author><name>Komga</name><uri>https://github.com/gotson/komga</uri></author>",
            "<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"self\" href=\"http://komga.local/opds/v1.2/series\"/>",
            "<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"start\" href=\"http://komga.local/opds/v1.2/catalog\"/>",
            "<entry>",
            "<title>series</title>",
            "<updated>2026-01-01T00:00:00Z</updated>",
            "<id>series-1</id>",
            "<content></content>",
            "<link type=\"application/atom+xml;profile=opds-catalog;kind=navigation\" rel=\"subsection\" href=\"http://komga.local/opds/v1.2/series/series-1\"/>",
            "</entry>",
            "</feed>"
        )
    );
}

#[tokio::test]
async fn libraries_route_returns_admin_snapshot_after_admin_login() {
    let app = komga_rust::app::build_router();

    let token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let json = libraries_json_for_token(&app, &token).await;

    assert_eq!(json, expected_snapshot("libraries-list-admin.json"));
    assert_eq!(json[0]["root"], "/library1");
}

#[tokio::test]
async fn libraries_route_returns_user_snapshot_with_hidden_root_after_user_login() {
    let app = komga_rust::app::build_router();

    let token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let json = libraries_json_for_token(&app, &token).await;

    assert_eq!(json, expected_snapshot("libraries-list-user.json"));
    assert_eq!(json[0]["root"], "");
}

#[tokio::test]
async fn libraries_route_returns_only_authorized_libraries_for_limited_user() {
    let app = komga_rust::app::build_router();

    let token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
    let json = libraries_json_for_token(&app, &token).await;

    assert_eq!(json, expected_snapshot("libraries-list-user.json"));
    assert_eq!(json.as_array().map(Vec::len), Some(1));
    assert_eq!(json[0]["id"], "1");
    assert_eq!(json[0]["root"], "");
}

#[tokio::test]
async fn libraries_route_uses_java_live_localdb_admin_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([
        {
            "id": "0PV0WX931SWP1",
            "name": "default",
            "root": "/tmp/komga-live-http-fixture",
            "importComicInfoBook": true,
            "importComicInfoSeries": true,
            "importComicInfoCollection": true,
            "importComicInfoReadList": true,
            "importComicInfoSeriesAppendVolume": true,
            "importEpubBook": true,
            "importEpubSeries": true,
            "importMylarSeries": true,
            "importLocalArtwork": true,
            "importBarcodeIsbn": true,
            "scanForceModifiedTime": false,
            "scanInterval": "EVERY_6H",
            "scanOnStartup": false,
            "scanCbx": true,
            "scanPdf": true,
            "scanEpub": true,
            "scanDirectoryExclusions": [],
            "repairExtensions": false,
            "convertToCbz": false,
            "emptyTrashAfterScan": false,
            "seriesCover": "FIRST",
            "hashFiles": true,
            "hashPages": false,
            "hashKoreader": false,
            "analyzeDimensions": true,
            "oneshotsDirectory": null,
            "unavailable": false
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
            let mut buffer = [0u8; 2048];
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
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nSet-Cookie: KOMGA-SESSION=java-admin-session; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}".to_string()
            } else {
                assert!(request.contains("GET /api/v1/libraries "));
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
    let json = libraries_json_for_token(&app, &token).await;

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

    assert_eq!(json, upstream_body);
    assert_eq!(json[0]["root"], "/tmp/komga-live-http-fixture");
    assert_ne!(json[0]["id"], "1");
}

#[tokio::test]
async fn libraries_route_returns_server_error_when_java_live_localdb_admin_fetch_fails() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = [0u8; 2048];
        let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
            .await
            .unwrap();
        let request = String::from_utf8_lossy(&buffer[..size]);

        assert!(request.contains("GET /api/v2/users/me "));

        let response = concat!(
            "HTTP/1.1 500 Internal Server Error\r\n",
            "Content-Type: application/json\r\n",
            "Content-Length: 18\r\n",
            "Connection: close\r\n",
            "\r\n",
            "{\"status\":500}"
        );
        tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes())
            .await
            .unwrap();
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
                .uri("/api/v1/libraries")
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

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn libraries_route_accepts_java_live_localdb_admin_token_bootstrap_without_cookie() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([]);
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(request_lower.contains("authorization: basic "));
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "X-Auth-Token: java-admin-token\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/libraries "));
                assert!(request_lower.contains("x-auth-token: java-admin-token"));
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
                .uri("/api/v1/libraries")
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
async fn libraries_route_uses_java_live_localdb_user_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!([]);
    let upstream_response = upstream_body.to_string();
    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        for step in 0..2 {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
            let size = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer)
                .await
                .unwrap();
            let request = String::from_utf8_lossy(&buffer[..size]);
            let request_lower = request.to_ascii_lowercase();

            let response = if step == 0 {
                assert!(request.contains("GET /api/v2/users/me "));
                assert!(
                    request_lower.contains("authorization: basic dxnlckblegftcgxllm9yzzp1c2vy"),
                    "unexpected bootstrap request: {request_lower}"
                );
                concat!(
                    "HTTP/1.1 200 OK\r\n",
                    "Content-Type: application/json\r\n",
                    "Set-Cookie: KOMGA-SESSION=java-user-session; Path=/; HttpOnly; SameSite=Lax\r\n",
                    "Content-Length: 2\r\n",
                    "Connection: close\r\n",
                    "\r\n",
                    "{}"
                )
                .to_string()
            } else {
                assert!(request.contains("GET /api/v1/libraries "));
                assert!(request_lower.contains("cookie: komga-session=java-user-session"));
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
    let token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
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
async fn series_route_returns_json_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/series")
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
    assert_eq!(json, expected_snapshot("series-list.json"));
}

#[tokio::test]
async fn series_route_uses_java_live_localdb_upstream_payload_instead_of_snapshot() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body: Value = serde_json::from_str(
        r#"{
            "content": [
                {
                    "id": "series-1",
                    "libraryId": "1",
                    "name": "series",
                    "url": "",
                    "created": "2026-03-21T13:39:06Z",
                    "lastModified": "2026-03-21T13:39:06Z",
                    "fileLastModified": "2024-01-02T03:04:05Z",
                    "booksCount": 0,
                    "booksReadCount": 0,
                    "booksUnreadCount": 0,
                    "booksInProgressCount": 0,
                    "metadata": {
                        "status": "ONGOING",
                        "statusLock": false,
                        "title": "series",
                        "titleLock": false,
                        "titleSort": "series",
                        "titleSortLock": false,
                        "summary": "",
                        "summaryLock": false,
                        "readingDirection": "",
                        "readingDirectionLock": false,
                        "publisher": "",
                        "publisherLock": false,
                        "ageRating": null,
                        "ageRatingLock": false,
                        "language": "",
                        "languageLock": false,
                        "genres": [],
                        "genresLock": false,
                        "tags": [],
                        "tagsLock": false,
                        "totalBookCount": null,
                        "totalBookCountLock": false,
                        "sharingLabels": [],
                        "sharingLabelsLock": false,
                        "links": [],
                        "linksLock": false,
                        "alternateTitles": [],
                        "alternateTitlesLock": false,
                        "created": "2026-03-21T13:39:06Z",
                        "lastModified": "2026-03-21T13:39:06Z"
                    },
                    "booksMetadata": {
                        "authors": [],
                        "tags": [],
                        "releaseDate": null,
                        "summary": "",
                        "summaryNumber": "",
                        "created": "2026-03-21T13:39:06Z",
                        "lastModified": "2026-03-21T13:39:06Z"
                    },
                    "deleted": false,
                    "oneshot": false
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
                assert!(request.contains("GET /api/v1/series "));
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
                .uri("/api/v1/series")
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
async fn series_route_applies_page_and_sort_query_shape() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/series?page=1&size=20&sort=metadata.titleSort,asc")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["number"], 1);
    assert_eq!(json["size"], 20);
    assert_eq!(json["first"], false);
    assert_eq!(json["last"], true);
    assert_eq!(json["numberOfElements"], 0);
    assert_eq!(json["empty"], true);
    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["totalPages"], 1);
    assert_eq!(json["sort"]["sorted"], true);
    assert_eq!(json["sort"]["unsorted"], false);
    assert_eq!(json["sort"]["empty"], false);
    assert_eq!(json["pageable"]["sort"]["sorted"], true);
    assert_eq!(json["pageable"]["sort"]["unsorted"], false);
    assert_eq!(json["pageable"]["sort"]["empty"], false);
    assert_eq!(json["content"], serde_json::json!([]));
}

#[tokio::test]
async fn series_route_applies_regex_and_authors_filters() {
    let app = komga_rust::app::build_router();

    let regex_filtered = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/series?search_regex=nomatch,title")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(regex_filtered.status(), StatusCode::OK);
    let regex_body = axum::body::to_bytes(regex_filtered.into_body(), usize::MAX)
        .await
        .unwrap();
    let regex_json: Value = serde_json::from_slice(&regex_body).unwrap();
    assert_eq!(regex_json["content"], serde_json::json!([]));
    assert_eq!(regex_json["totalElements"], 0);

    let authors_filtered = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/series?authors=John%20Doe,writer")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(authors_filtered.status(), StatusCode::OK);
    let authors_body = axum::body::to_bytes(authors_filtered.into_body(), usize::MAX)
        .await
        .unwrap();
    let authors_json: Value = serde_json::from_slice(&authors_body).unwrap();
    assert_eq!(authors_json["content"], serde_json::json!([]));
    assert_eq!(authors_json["totalElements"], 0);
}

#[tokio::test]
async fn series_list_route_applies_post_search_and_sort_shape() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"fullTextSearch":"nomatch"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["content"], serde_json::json!([]));
    assert_eq!(json["totalElements"], 0);
    assert_eq!(json["sort"]["sorted"], true);
    assert_eq!(json["sort"]["unsorted"], false);
    assert_eq!(json["sort"]["empty"], false);
}

#[tokio::test]
async fn series_list_route_matches_search_query_case_for_seeded_dataset() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"fullTextSearch":"series"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["numberOfElements"], 1);
    assert_eq!(json["content"][0]["id"], "series-1");
}

#[tokio::test]
async fn series_list_route_matches_search_ordering_case_with_deterministic_pagination() {
    let app = komga_rust::app::build_router();

    let first_page = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=1&sort=metadata.titleSort,asc")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"fullTextSearch":"series"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(first_page.status(), StatusCode::OK);
    let first_page_body = axum::body::to_bytes(first_page.into_body(), usize::MAX)
        .await
        .unwrap();
    let first_page_json: Value = serde_json::from_slice(&first_page_body).unwrap();

    assert_eq!(first_page_json["number"], 0);
    assert_eq!(first_page_json["size"], 1);
    assert_eq!(first_page_json["totalElements"], 1);
    assert_eq!(first_page_json["totalPages"], 1);
    assert_eq!(first_page_json["numberOfElements"], 1);
    assert_eq!(first_page_json["sort"]["sorted"], true);
    assert_eq!(first_page_json["sort"]["unsorted"], false);
    assert_eq!(first_page_json["content"][0]["id"], "series-1");

    let second_page = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=1&size=1&sort=metadata.titleSort,asc")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"fullTextSearch":"series"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(second_page.status(), StatusCode::OK);
    let second_page_body = axum::body::to_bytes(second_page.into_body(), usize::MAX)
        .await
        .unwrap();
    let second_page_json: Value = serde_json::from_slice(&second_page_body).unwrap();

    assert_eq!(second_page_json["number"], 1);
    assert_eq!(second_page_json["numberOfElements"], 0);
    assert_eq!(second_page_json["totalElements"], 1);
    assert_eq!(second_page_json["content"], serde_json::json!([]));
}

#[tokio::test]
async fn series_list_route_exposes_shadow_search_ownership_marker_without_changing_results() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .header("X-Komga-Compat-Search-Ownership", "shadow-java-writer")
                .body(Body::from(
                    r#"{"fullTextSearch":"series","ownership":"shadow"}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get("x-komga-compat-search-ownership")
            .and_then(|value| value.to_str().ok()),
        Some("shadow-java-writer"),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["content"][0]["id"], "series-1");
}

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
async fn login_set_cookie_returns_session_headers_when_authorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/login/set-cookie")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    assert!(response.headers().get("x-auth-token").is_none());

    let set_cookie = response.headers().get(header::SET_COOKIE).unwrap();
    let cookie = set_cookie.to_str().unwrap();
    assert!(cookie.contains("KOMGA-SESSION="));
    assert!(cookie.contains("Path=/"));
}

#[tokio::test]
async fn users_me_requires_credentials() {
    let app = komga_rust::app::build_router();

    let missing = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);

    let empty = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(empty.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_me_returns_auth_token_header_and_json_for_basic_auth() {
    let app = komga_rust::app::build_router();

    let admin = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    "Basic YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(admin.status(), StatusCode::OK);
    assert!(
        !admin
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    let admin_cookie = admin
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(admin_cookie.contains("KOMGA-SESSION="));

    let admin_body = axum::body::to_bytes(admin.into_body(), usize::MAX)
        .await
        .unwrap();
    let admin_json: Value = serde_json::from_slice(&admin_body).unwrap();
    assert_eq!(admin_json["id"], "admin");
    assert_eq!(admin_json["email"], "admin@example.org");

    let user = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(user.status(), StatusCode::OK);
    assert!(
        !user
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    let user_cookie = user
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(user_cookie.contains("KOMGA-SESSION="));

    let user_body = axum::body::to_bytes(user.into_body(), usize::MAX)
        .await
        .unwrap();
    let user_json: Value = serde_json::from_slice(&user_body).unwrap();
    assert_eq!(user_json["id"], "0PV32486S7X3J");
    assert_eq!(user_json["email"], "user@example.org");
    assert_eq!(
        user_json["roles"],
        serde_json::json!(["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"])
    );
    assert_eq!(user_json["sharedAllLibraries"], true);
    assert_eq!(user_json["sharedLibrariesIds"], serde_json::json!([]));
    assert_eq!(user_json["labelsAllow"], serde_json::json!([]));
    assert_eq!(user_json["labelsExclude"], serde_json::json!([]));
    assert!(user_json["ageRestriction"].is_null());

    let limited = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(
                    header::AUTHORIZATION,
                    "Basic bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(limited.status(), StatusCode::OK);
    assert!(
        !limited
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );
    let limited_cookie = limited
        .headers()
        .get(header::SET_COOKIE)
        .unwrap()
        .to_str()
        .unwrap();
    assert!(limited_cookie.contains("KOMGA-SESSION="));

    let limited_body = axum::body::to_bytes(limited.into_body(), usize::MAX)
        .await
        .unwrap();
    let limited_json: Value = serde_json::from_slice(&limited_body).unwrap();
    assert_eq!(limited_json["id"], "1PXGX4XP02A26");
    assert_eq!(limited_json["email"], "limited@example.org");
    assert_eq!(
        limited_json["roles"],
        serde_json::json!(["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"])
    );
    assert_eq!(limited_json["sharedAllLibraries"], false);
    assert_eq!(limited_json["sharedLibrariesIds"], serde_json::json!(["1"]));
    assert_eq!(limited_json["labelsAllow"], serde_json::json!([]));
    assert_eq!(limited_json["labelsExclude"], serde_json::json!([]));
    assert!(limited_json["ageRestriction"].is_null());

    let invalid = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, "Basic aW52YWxpZDp0b2tlbg==")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(invalid.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn users_me_accepts_valid_api_key_with_uppercase_header_and_sets_session_cookie() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("X-API-Key", "compat-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let session_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("valid api key should issue session cookie")
        .to_str()
        .unwrap();
    assert!(session_cookie.contains("KOMGA-SESSION="));
    assert!(session_cookie.contains("HttpOnly"));

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["email"], "user@example.org");
}

#[tokio::test]
async fn users_me_accepts_valid_api_key_with_lowercase_header_and_sets_session_cookie() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("x-api-key", "compat-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let session_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("valid api key should issue session cookie")
        .to_str()
        .unwrap();
    assert!(session_cookie.contains("KOMGA-SESSION="));
    assert!(session_cookie.contains("HttpOnly"));

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["email"], "user@example.org");
}

#[tokio::test]
async fn users_me_rejects_invalid_api_key_with_unauthorized() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header("x-api-key", "invalid-api-key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["error"], "Unauthorized");
    assert_eq!(json["message"], "Unauthorized");
    assert_eq!(json["path"], "/api/v2/users/me");
    assert_eq!(json["status"], 401);
    assert!(
        json["timestamp"]
            .as_str()
            .is_some_and(|value| !value.is_empty())
    );
}

#[tokio::test]
async fn users_me_with_remember_me_returns_session_and_remember_me_cookies() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me?remember-me=true")
                .header(header::AUTHORIZATION, "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert!(response.headers().get("x-auth-token").is_none());

    let set_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();

    assert!(
        set_cookies
            .iter()
            .any(|cookie| cookie.contains("KOMGA-SESSION="))
    );
    assert!(
        set_cookies
            .iter()
            .any(|cookie| cookie.contains("komga-remember-me="))
    );
    assert_eq!(set_cookies.len(), 2);

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "0PV32486S7X3J");
    assert_eq!(json["email"], "user@example.org");
    assert_eq!(
        json["roles"],
        serde_json::json!(["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"])
    );
    assert_eq!(json["sharedAllLibraries"], true);
    assert_eq!(json["sharedLibrariesIds"], serde_json::json!([]));
    assert_eq!(json["labelsAllow"], serde_json::json!([]));
    assert_eq!(json["labelsExclude"], serde_json::json!([]));
    assert!(json["ageRestriction"].is_null());
}

#[tokio::test]
async fn users_me_with_remember_me_and_empty_auth_token_returns_exchange_header() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me?remember-me=true")
                .header(header::AUTHORIZATION, "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy")
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let x_auth_token = response
        .headers()
        .get("x-auth-token")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(!x_auth_token.is_empty());

    let set_cookies: Vec<_> = response
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|value| value.to_str().unwrap())
        .collect();

    assert_eq!(set_cookies.len(), 1);
    assert!(set_cookies[0].contains("komga-remember-me="));
    assert!(set_cookies[0].contains("Max-Age="));
    assert!(set_cookies[0].contains("Expires="));
    assert!(response.headers().get("x-auth-token").is_some());

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], "0PV32486S7X3J");
    assert_eq!(json["email"], "user@example.org");
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

#[tokio::test]
async fn opds_catalog_route_issues_auth_challenge_when_unauthenticated() {
    assert_opds_catalog_challenge_for_host("localhost").await;
    assert_opds_catalog_challenge_for_host("127.0.0.1").await;
}

#[tokio::test]
async fn opds_auth_route_returns_auth_document() {
    assert_opds_auth_for_host("localhost").await;
    assert_opds_auth_for_host("127.0.0.1").await;
}

#[tokio::test]
async fn opds_manifest_route_returns_snapshot_json() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-publication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_opds_snapshot("opds-v2-manifest.json"));
}

#[tokio::test]
async fn opds_manifest_route_uses_request_host_in_java_live_localdb() {
    assert_java_live_manifest_for_host("localhost").await;
    assert_java_live_manifest_for_host("127.0.0.1").await;
}

#[tokio::test]
async fn opds_manifest_route_uses_java_live_upstream_manifest_when_configured() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let upstream_body = serde_json::json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "images": [],
        "landmarks": [],
        "links": [
            {
                "href": "http://komga.local/opds/v2/books/book-1/manifest",
                "properties": {
                    "authenticate": {
                        "href": "http://komga.local/opds/v2/auth",
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "self",
                "type": "application/divina+json",
            }
        ],
        "metadata": {
            "title": "book.cbr",
            "modified": "2026-03-21T09:08:28-04:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
            "published": "2024-01-01",
        },
        "pageList": [],
        "readingOrder": [
            {
                "href": "http://komga.local/opds/v2/books/book-1/pages/1?contentNegotiation=false",
                "type": "image/png",
                "width": 1,
                "height": 1,
            }
        ],
        "resources": [],
        "toc": [],
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
                assert!(request.contains("GET /opds/v2/books/book-1/manifest "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/opds-publication+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header("X-Auth-Token", "dummy-token")
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
        "application/opds-publication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, upstream_body);
}

#[tokio::test]
async fn opds_manifest_route_rewrites_java_live_upstream_urls_to_request_host() {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let listener = tokio::net::TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .unwrap();
    let address = listener.local_addr().unwrap();
    let upstream_base = format!("http://{address}");
    let upstream_body = serde_json::json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "images": [],
        "landmarks": [],
        "links": [
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/manifest"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "self",
                "type": "application/divina+json",
            },
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/file"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "http://opds-spec.org/acquisition",
                "type": "application/vnd.comicbook+zip",
            },
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/progression"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "rel": "http://www.cantook.com/api/progression",
                "type": "application/vnd.readium.progression+json",
            }
        ],
        "metadata": {
            "title": "book.cbr",
            "modified": "2026-03-21T09:19:18-04:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
            "published": "2024-01-01",
            "belongsTo": {
                "series": [
                    {
                        "name": "series",
                        "position": 1.0,
                        "links": [
                            {
                                "href": format!("{upstream_base}/opds/v2/series/series-1"),
                                "type": "application/opds+json",
                            }
                        ],
                    }
                ]
            }
        },
        "pageList": [],
        "readingOrder": [
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/pages/1?contentNegotiation=false"),
                "type": "image/png",
                "width": 1,
                "height": 1,
            }
        ],
        "resources": [
            {
                "href": format!("{upstream_base}/opds/v2/books/book-1/thumbnail"),
                "properties": {
                    "authenticate": {
                        "href": format!("{upstream_base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                },
                "type": "image/jpeg",
            }
        ],
        "toc": [],
    });
    let upstream_response = upstream_body.to_string();
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
                assert!(request.contains("GET /opds/v2/books/book-1/manifest "));
                assert!(request_lower.contains("cookie: komga-session=java-admin-session"));
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/opds-publication+json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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
    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header(header::HOST, "komga.local")
                .header("X-Auth-Token", "dummy-token")
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

    assert_eq!(
        json["links"][0]["href"],
        "http://komga.local/opds/v2/books/book-1/manifest"
    );
    assert_eq!(
        json["links"][0]["properties"]["authenticate"]["href"],
        "http://komga.local/opds/v2/auth"
    );
    assert_eq!(
        json["links"][1]["href"],
        "http://komga.local/opds/v2/books/book-1/file"
    );
    assert_eq!(
        json["links"][2]["href"],
        "http://komga.local/opds/v2/books/book-1/progression"
    );
    assert_eq!(
        json["metadata"]["belongsTo"]["series"][0]["links"][0]["href"],
        "http://komga.local/opds/v2/series/series-1"
    );
    assert_eq!(
        json["readingOrder"][0]["href"],
        "http://komga.local/opds/v2/books/book-1/pages/1?contentNegotiation=false"
    );
    assert_eq!(
        json["resources"][0]["href"],
        "http://komga.local/opds/v2/books/book-1/thumbnail"
    );
}

#[tokio::test]
async fn opds_v1_series_route_returns_atom_feed_for_authorized_user() {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v1.2/series")
                .header("X-Auth-Token", "komga-user-token")
                .header(header::HOST, "localhost")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/atom+xml"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let xml = String::from_utf8(body.to_vec()).unwrap();

    assert!(xml.contains("<id>allSeries</id>"));
    assert!(xml.contains("<title>All series</title>"));
    assert!(xml.contains("href=\"http://localhost/opds/v1.2/series\""));
    assert!(xml.contains("href=\"http://localhost/opds/v1.2/catalog\""));
    assert!(!xml.contains("<?xml"));
    assert!(!xml.contains("<entry>"));
}

fn expected_snapshot(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/test/resources/compatibility-snapshots/rest")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

async fn session_token_for_basic_auth<S>(app: &S, basic_auth: &str) -> String
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

fn expected_opds_snapshot(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/test/resources/compatibility-snapshots/opds")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

async fn assert_opds_auth_for_host(host: &str) {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/auth")
                .header(header::HOST, host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-authentication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_opds_auth(host));
}

async fn assert_opds_catalog_challenge_for_host(host: &str) {
    let app = komga_rust::app::build_router();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/catalog")
                .header(header::HOST, host)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get(header::WWW_AUTHENTICATE).unwrap(),
        "Basic realm=\"Realm\""
    );
    assert_eq!(
        response
            .headers()
            .get(header::LINK)
            .unwrap()
            .to_str()
            .unwrap(),
        format!(
            "<http://{host}/opds/v2/auth>; rel=\"http://opds-spec.org/auth/document\"; type=\"application/opds-authentication+json\""
        )
    );
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-authentication+json;charset=UTF-8"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_opds_auth(host));
}

fn expected_opds_auth(host: &str) -> Value {
    let base = format!("http://{host}");

    serde_json::json!({
        "authentication": [
            {
                "type": "http://opds-spec.org/auth/basic",
                "labels": {
                    "login": "Email",
                    "password": "Password",
                },
            }
        ],
        "title": "Komga",
        "id": format!("{base}/opds/v2/auth"),
        "description": "Enter your email and password to authenticate.",
        "links": [
            {
                "rel": "help",
                "href": "https://komga.org",
            },
            {
                "rel": "logo",
                "href": format!("{base}/android-chrome-512x512.png"),
            }
        ]
    })
}

fn expected_java_live_manifest(host: &str) -> Value {
    let base = format!("http://{host}");

    serde_json::json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": {
            "title": "book.cbr",
            "modified": "2026-03-21T09:08:28-04:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
            "published": "2024-01-01",
            "belongsTo": {
                "series": [
                    {
                        "name": "series",
                        "position": 1.0,
                        "links": [
                            {
                                "href": format!("{base}/opds/v2/series/series-1"),
                                "type": "application/opds+json",
                            }
                        ],
                    }
                ]
            }
        },
        "links": [
            {
                "rel": "self",
                "href": format!("{base}/opds/v2/books/book-1/manifest"),
                "type": "application/divina+json",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": format!("{base}/opds/v2/books/book-1/file"),
                "type": "application/vnd.comicbook+zip",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            },
            {
                "rel": "http://www.cantook.com/api/progression",
                "href": format!("{base}/opds/v2/books/book-1/progression"),
                "type": "application/vnd.readium.progression+json",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            }
        ],
        "images": [],
        "readingOrder": [
            {
                "href": format!("{base}/opds/v2/books/book-1/pages/1?contentNegotiation=false"),
                "type": "image/png",
                "width": 1,
                "height": 1,
            }
        ],
        "resources": [
            {
                "href": format!("{base}/opds/v2/books/book-1/thumbnail"),
                "type": "image/jpeg",
                "properties": {
                    "authenticate": {
                        "href": format!("{base}/opds/v2/auth"),
                        "type": "application/opds-authentication+json",
                    }
                }
            }
        ],
        "toc": [],
        "landmarks": [],
        "pageList": [],
    })
}

async fn assert_java_live_manifest_for_host(host: &str) {
    let _java_live_base_url_env_guard = JAVA_LIVE_BASE_URL_ENV_LOCK.lock().await;

    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/opds/v2/books/book-1/manifest")
                .header("Host", host)
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/opds-publication+json"
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json, expected_java_live_manifest(host));
}

async fn assert_series_and_books_urls(
    app: axum::Router,
    expected_series_url: &str,
    expected_book_url: &str,
) {
    let series_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/series")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let series_body = axum::body::to_bytes(series_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let series_json: Value = serde_json::from_slice(&series_body).unwrap();
    assert_eq!(series_json["content"][0]["url"], expected_series_url);

    let books_response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/books")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let books_body = axum::body::to_bytes(books_response.into_body(), usize::MAX)
        .await
        .unwrap();
    let books_json: Value = serde_json::from_slice(&books_body).unwrap();
    assert_eq!(books_json["content"][0]["url"], expected_book_url);
}
