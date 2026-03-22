use super::*;

#[tokio::test]
async fn supported_discovery_shapes_use_native_path() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;

    let libraries_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", &admin_token)
                .header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        libraries_response
            .headers()
            .get(NATIVE_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let series_response = series_list_response_for_token(
        &app,
        &admin_token,
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc",
        r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#,
        true,
    )
    .await;
    assert_eq!(
        series_response
            .headers()
            .get(NATIVE_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let books_list_response = books_list_response_for_token(
        &app,
        &admin_token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        r#"{"condition":{"type":"LibraryId","operator":"is","value":"1"}}"#,
        true,
    )
    .await;
    assert_eq!(
        books_list_response
            .headers()
            .get(NATIVE_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let books_latest_response = books_latest_response_for_token(
        &app,
        &admin_token,
        "/api/v1/books/latest?page=0&size=20",
        true,
    )
    .await;
    assert_eq!(
        books_latest_response
            .headers()
            .get(NATIVE_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );
}

#[tokio::test]
async fn unsupported_discovery_shapes_emit_non_native_marker() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;

    let response = books_latest_response_for_token(
        &app,
        &admin_token,
        "/api/v1/books/latest?page=0&size=20&sort=metadata.title,asc",
        true,
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(NATIVE_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some("shadow-java-writer"),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["_compat"]["discoveryOwnership"], "non-native");
    assert_eq!(json["_compat"]["reason"], "unsupported-request-shape");
    assert_eq!(
        json["_compat"]["shape"],
        "UnsupportedBookSort(metadata.title,asc)",
    );
}
