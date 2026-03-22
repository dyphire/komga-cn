use super::*;

#[tokio::test]
async fn excluded_oneshot_query_parameter_emits_shadow_marker() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let response = get_response(&app, &token, "/api/v1/series/series-oneshot?oneshot=true").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_shadow_marker(&response, "series detail oneshot query parameter");

    let json = response_json(response).await;
    assert_eq!(json["id"], "series-oneshot");
    assert_eq!(json["_compat"]["discoveryOwnership"], "non-native");
    assert_eq!(
        json["_compat"]["shape"],
        "UnsupportedSeriesFilter(oneshot-query-parameter)",
    );
}

#[tokio::test]
async fn excluded_oneshot_branches_emit_shadow_marker() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;
    let expected = BTreeSet::from([
        "oneshot bootstrap widening",
        "media delivery",
        "reader download adjacency",
        "progress routes",
        "SSE live-refresh",
    ]);
    let mut observed = BTreeSet::new();

    let widened_bootstrap = post_response(
        &app,
        &token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc",
        r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-oneshot"}]}}"#,
        Some("shadow-java-writer"),
    )
    .await;
    assert_eq!(widened_bootstrap.status(), StatusCode::OK);
    assert_shadow_marker(&widened_bootstrap, "oneshot bootstrap widening");
    observed.insert("oneshot bootstrap widening");

    let native_detail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(native_detail.status(), StatusCode::OK);
    assert!(
        native_detail
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .is_none(),
        "native detail payload must stay unmarked even when adjacent excluded branches are visible in UI",
    );
    let native_detail_body = axum::body::to_bytes(native_detail.into_body(), usize::MAX)
        .await
        .unwrap();
    let native_detail_json: Value = serde_json::from_slice(&native_detail_body).unwrap();
    assert_eq!(native_detail_json["url"], "book.cbr");

    let book_pages = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(book_pages.status(), StatusCode::OK);
    assert_shadow_marker(&book_pages, "book pages inventory");
    observed.insert("media delivery");

    let page_asset = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page_asset.status(), StatusCode::OK);
    assert_shadow_marker(&page_asset, "book page asset");

    let page_thumbnail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/pages/1/thumbnail")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(page_thumbnail.status(), StatusCode::OK);
    assert_shadow_marker(&page_thumbnail, "book page thumbnail");

    let book_thumbnail = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/thumbnail")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(book_thumbnail.status(), StatusCode::NOT_FOUND);
    assert_shadow_marker(&book_thumbnail, "book thumbnail");

    let download = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1/file")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(download.status(), StatusCode::OK);
    assert_shadow_marker(&download, "book file download");
    assert!(
        download
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .is_some()
    );
    observed.insert("reader download adjacency");

    let read_progress_patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"page\":10}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read_progress_patch.status(), StatusCode::NO_CONTENT);
    assert_shadow_marker(&read_progress_patch, "read-progress patch");
    observed.insert("progress routes");

    let read_progress_delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(read_progress_delete.status(), StatusCode::NO_CONTENT);
    assert_shadow_marker(&read_progress_delete, "read-progress delete");

    let progression = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/progression")
                .header("X-Auth-Token", &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"modified":"2024-01-01T00:00:00Z","device":"compat-client","locator":{"href":"OEBPS/chapter-1.xhtml","type":"application/xhtml+xml","locations":{"progression":0.3}}}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(progression.status(), StatusCode::NO_CONTENT);
    assert_shadow_marker(&progression, "progression patch");

    let live_refresh = app
        .oneshot(
            Request::builder()
                .uri("/sse/v1/events")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(live_refresh.status(), StatusCode::OK);
    assert_shadow_marker(&live_refresh, "SSE live-refresh");
    assert_eq!(
        live_refresh.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/event-stream",
    );
    observed.insert("SSE live-refresh");

    assert_eq!(expected, observed);
}

#[tokio::test]
async fn embedded_read_progress_is_preserved_without_owning_progress_routes() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let before = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(before.status(), StatusCode::OK);
    let before_body = axum::body::to_bytes(before.into_body(), usize::MAX)
        .await
        .unwrap();
    let before_json: Value = serde_json::from_slice(&before_body).unwrap();
    assert_eq!(before_json["readProgress"]["page"], 7);
    assert_eq!(before_json["readProgress"]["completed"], false);
    assert_eq!(before_json["readProgress"]["deviceId"], "device-android");

    let patch = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("X-Auth-Token", &token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{\"page\":10}"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(patch.status(), StatusCode::NO_CONTENT);

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/books/book-1")
                .header("X-Auth-Token", &token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(after.status(), StatusCode::OK);
    let after_body = axum::body::to_bytes(after.into_body(), usize::MAX)
        .await
        .unwrap();
    let after_json: Value = serde_json::from_slice(&after_body).unwrap();
    assert_eq!(after_json["readProgress"]["page"], 7);
    assert_eq!(
        after_json["readProgress"]["readDate"],
        "2024-01-04T03:04:05Z"
    );
    assert_eq!(after_json["readProgress"]["deviceName"], "Android");
}
