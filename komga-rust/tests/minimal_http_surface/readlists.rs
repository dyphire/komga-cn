use super::*;

#[tokio::test]
async fn readlist_create_update_delete_routes_are_available() {
    let app = komga_rust::app::build_router();

    let create = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"name":"readlist","summary":"summary","bookIds":["book-1"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(create.status(), StatusCode::OK);
    let create_body = axum::body::to_bytes(create.into_body(), usize::MAX)
        .await
        .unwrap();
    let create_json: Value = serde_json::from_slice(&create_body).unwrap();
    assert_eq!(create_json["id"], "readlist-created");
    assert_eq!(create_json["name"], "readlist");

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/readlists/readlist-2")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"name":"updated"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let delete = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/readlists/readlist-2")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
async fn readlist_thumbnail_routes_return_expected_contracts() {
    let app = komga_rust::app::build_router();

    let thumbnail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-1/thumbnail")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(thumbnail.status(), StatusCode::OK);
    assert_eq!(
        thumbnail.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/jpeg"
    );

    let thumbnails = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(thumbnails.status(), StatusCode::OK);
    let thumbnails_body = axum::body::to_bytes(thumbnails.into_body(), usize::MAX)
        .await
        .unwrap();
    let thumbnails_json: Value = serde_json::from_slice(&thumbnails_body).unwrap();
    assert_eq!(thumbnails_json[0]["id"], "thumbnail-1");

    let unknown = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-1/thumbnails/does-not-exist")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn readlist_tachiyomi_routes_return_expected_contracts() {
    let app = komga_rust::app::build_router();

    let get_progress = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_progress.status(), StatusCode::OK);
    let get_body = axum::body::to_bytes(get_progress.into_body(), usize::MAX)
        .await
        .unwrap();
    let get_json: Value = serde_json::from_slice(&get_body).unwrap();
    assert_eq!(get_json["booksCount"], 5);
    assert_eq!(get_json["booksReadCount"], 0);

    let put_progress = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/read-progress/tachiyomi")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"lastBookRead":2}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_progress.status(), StatusCode::NO_CONTENT);

    let put_missing = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/does-not-exist/read-progress/tachiyomi")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"lastBookRead":2}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(put_missing.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn readlist_file_and_comicrack_routes_are_available() {
    let app = komga_rust::app::build_router();

    let file = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-1/file")
                .header("X-Auth-Token", "dummy-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(file.status(), StatusCode::OK);
    assert_eq!(
        file.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/zip"
    );
    assert!(file.headers().get(header::CONTENT_DISPOSITION).is_some());

    let comicrack = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("X-Auth-Token", "dummy-token")
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=komga")
                .body(Body::from("--komga--"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(comicrack.status(), StatusCode::OK);
}

#[tokio::test]
async fn java_live_profile_keeps_readlists_family_on_rust_runtime_surface() {
    let app =
        komga_rust::app::build_router_with_profile(komga_rust::app::CompatProfile::JavaLiveLocaldb);
    let token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;

    let readlists = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readlists.status(), StatusCode::OK);
    assert_eq!(
        readlists
            .headers()
            .get("x-komga-compat-search-ownership")
            .and_then(|value| value.to_str().ok()),
        Some("native-rust-owned"),
    );
    let readlists_body = axum::body::to_bytes(readlists.into_body(), usize::MAX)
        .await
        .unwrap();
    let readlists_json: Value = serde_json::from_slice(&readlists_body).unwrap();
    assert!(readlists_json.get("_compat").is_none());

    let readlist_detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-2")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readlist_detail.status(), StatusCode::OK);
    assert!(
        readlist_detail
            .headers()
            .get("x-komga-compat-search-ownership")
            .is_none()
    );
    let readlist_detail_body = axum::body::to_bytes(readlist_detail.into_body(), usize::MAX)
        .await
        .unwrap();
    let readlist_detail_json: Value = serde_json::from_slice(&readlist_detail_body).unwrap();
    assert_eq!(readlist_detail_json["id"], "readlist-2");
    assert!(readlist_detail_json.get("_compat").is_none());

    let readlist_books = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-2/books?unpaged=true")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(readlist_books.status(), StatusCode::OK);
    assert_eq!(
        readlist_books
            .headers()
            .get("x-komga-compat-search-ownership")
            .and_then(|value| value.to_str().ok()),
        Some("native-rust-owned"),
    );
    let readlist_books_body = axum::body::to_bytes(readlist_books.into_body(), usize::MAX)
        .await
        .unwrap();
    let readlist_books_json: Value = serde_json::from_slice(&readlist_books_body).unwrap();
    assert!(readlist_books_json.get("_compat").is_none());

    let previous = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-2/books/book-1/previous")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(previous.status(), StatusCode::NOT_FOUND);
    assert!(
        previous
            .headers()
            .get("x-komga-compat-search-ownership")
            .is_none()
    );

    let next = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/readlists/readlist-2/books/book-1/next")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(next.status(), StatusCode::OK);
    assert!(
        next.headers()
            .get("x-komga-compat-search-ownership")
            .is_none()
    );
    let next_body = axum::body::to_bytes(next.into_body(), usize::MAX)
        .await
        .unwrap();
    let next_json: Value = serde_json::from_slice(&next_body).unwrap();
    assert_eq!(next_json["id"], "book-2");
    assert!(next_json.get("_compat").is_none());
}
