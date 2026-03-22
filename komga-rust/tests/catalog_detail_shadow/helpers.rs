use super::*;

#[tokio::test]
async fn page_scoped_books_list_is_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, ADMIN_BASIC_AUTH).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc")
                .header("X-Auth-Token", &token)
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-1"}]}}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["content"][0]["id"], "book-1");
}

#[tokio::test]
async fn oneshot_books_list_shape_is_native_owned() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list")
                .header("X-Auth-Token", &token)
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(page_content_ids(&json), vec!["book-oneshot"]);
    assert!(json.get("_compat").is_none());
}

pub(super) async fn browse_oneshot_happy_path_uses_native_bootstrap_shape() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let series_detail = get_response(&app, &token, "/api/v1/series/series-oneshot").await;
    assert_eq!(series_detail.status(), StatusCode::OK);
    assert_native_owned(&series_detail, "oneshot series detail");
    let series_detail_json = response_json(series_detail).await;
    assert_eq!(series_detail_json["id"], "series-oneshot");
    assert!(series_detail_json.get("_compat").is_none());

    let collections = get_response(&app, &token, "/api/v1/series/series-oneshot/collections").await;
    assert_eq!(collections.status(), StatusCode::OK);
    assert_native_owned(&collections, "oneshot series collections");
    let collections_json = response_json(collections).await;
    assert!(collections_json.is_array());
    assert!(collections_json.get("_compat").is_none());

    let bootstrap = post_response(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
        None,
    )
    .await;
    assert_eq!(bootstrap.status(), StatusCode::OK);
    assert_eq!(
        bootstrap
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "oneshot bootstrap books/list should return native marker without request marker hack",
    );
    let bootstrap_json = response_json(bootstrap).await;
    assert_eq!(page_content_ids(&bootstrap_json), vec!["book-oneshot"]);
    assert!(bootstrap_json.get("_compat").is_none());

    let readlists = get_response(&app, &token, "/api/v1/books/book-oneshot/readlists").await;
    assert_eq!(readlists.status(), StatusCode::OK);
    assert_native_owned(&readlists, "oneshot book readlists");
    let readlists_json = response_json(readlists).await;
    assert!(readlists_json.is_array());
    assert!(readlists_json.get("_compat").is_none());
}

#[tokio::test]
async fn phase3_phase4_owned_routes_do_not_regress_with_oneshot_bootstrap() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let oneshot_bootstrap = post_response(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
        None,
    )
    .await;
    assert_eq!(oneshot_bootstrap.status(), StatusCode::OK);
    assert_eq!(
        oneshot_bootstrap
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "oneshot bootstrap should stay native without request marker hack",
    );

    let phase3_books = post_response(
        &app,
        &token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.numberSort,asc",
        r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-1"}]}}"#,
        Some(NATIVE_OWNERSHIP_MARKER),
    )
    .await;
    assert_eq!(phase3_books.status(), StatusCode::OK);
    assert_eq!(
        phase3_books
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "phase3 page-scoped books/list marker propagation must stay unchanged",
    );
    let phase3_books_json = response_json(phase3_books).await;
    assert_eq!(phase3_books_json["content"][0]["id"], "book-1");
    assert!(phase3_books_json.get("_compat").is_none());

    let phase4_readlist_books = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books?unpaged=true",
    )
    .await;
    assert_eq!(phase4_readlist_books.status(), StatusCode::OK);
    assert_eq!(
        phase4_readlist_books
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "phase4 readlist unpaged route marker propagation must stay unchanged",
    );
    let phase4_readlist_books_json = response_json(phase4_readlist_books).await;
    assert_eq!(
        page_content_ids(&phase4_readlist_books_json),
        vec!["book-1", "book-2"]
    );
    assert!(phase4_readlist_books_json.get("_compat").is_none());
}
