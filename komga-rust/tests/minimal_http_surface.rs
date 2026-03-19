use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use std::path::PathBuf;
use tower::ServiceExt;

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
async fn libraries_route_uses_empty_root_in_java_live_localdb() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["root"], "");
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
async fn books_latest_route_uses_sorted_page_metadata_in_java_live_localdb() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["sort"]["sorted"], true);
    assert_eq!(json["sort"]["unsorted"], false);
    assert_eq!(json["sort"]["empty"], false);
    assert_eq!(json["pageable"]["sort"]["sorted"], true);
    assert_eq!(json["pageable"]["sort"]["unsorted"], false);
    assert_eq!(json["pageable"]["sort"]["empty"], false);
}

#[tokio::test]
async fn series_and_books_urls_follow_selected_compat_profile() {
    let default_app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::SnapshotAligned);
    assert_series_and_books_urls(default_app, "", "book.cbr").await;

    let live_app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    assert_series_and_books_urls(live_app, "", "book.cbr").await;
}

#[tokio::test]
async fn books_route_uses_live_localdb_media_shape() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

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

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    let book = &json["content"][0];

    assert_eq!(book["url"], "book.cbr");
    assert_eq!(book["fileLastModified"], "2024-01-02T08:04:05Z");
    assert_eq!(book["media"]["status"], "READY");
    assert_eq!(book["media"]["mediaType"], "application/zip");
    assert_eq!(book["media"]["pagesCount"], 1);
    assert_eq!(book["media"]["mediaProfile"], "DIVINA");
}

#[tokio::test]
async fn book_pages_route_uses_java_live_localdb_page_metadata() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);

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
    assert_eq!(
        json,
        serde_json::json!([
            {
                "number": 1,
                "fileName": "komga.png",
                "mediaType": "image/png",
                "width": null,
                "height": null,
                "sizeBytes": 0,
                "size": "0 B",
            }
        ])
    );
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
async fn users_me_returns_auth_token_header_and_json() {
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

    assert_eq!(missing.status(), StatusCode::OK);
    assert!(
        !missing
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );

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

    assert_eq!(empty.status(), StatusCode::OK);
    assert!(
        !empty
            .headers()
            .get("x-auth-token")
            .unwrap()
            .to_str()
            .unwrap()
            .is_empty()
    );

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
    assert_eq!(user_json["id"], "0PTTX3XD04FM0");
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
    assert_eq!(json["id"], "0PTTX3XD04FM0");
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
    assert_eq!(json["id"], "0PTTX3XD04FM0");
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
    assert!(disposition.contains("book.pdf"));
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

fn expected_snapshot(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../komga/src/test/resources/compatibility-snapshots/rest")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
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
        response.headers().get(header::LINK).unwrap().to_str().unwrap(),
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
            "modified": "2024-01-01T22:04:05-05:00",
            "conformsTo": "https://readium.org/webpub-manifest/profiles/divina",
            "numberOfPages": 1,
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
