use super::*;

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
