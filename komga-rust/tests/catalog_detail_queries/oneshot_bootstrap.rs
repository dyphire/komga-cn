use super::{
    NATIVE_OWNERSHIP_MARKER, RESTRICTED_BASIC_AUTH, SEARCH_OWNERSHIP_HEADER, StatusCode,
    USER_BASIC_AUTH, page_content_ids, post_books_list, response_json,
    session_token_for_basic_auth,
};

#[tokio::test(flavor = "multi_thread")]
async fn oneshot_bootstrap_requires_visible_oneshot_series() {
    let app = komga_rust::app::build_router();
    let user_token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;
    let restricted_token = session_token_for_basic_auth(&app, RESTRICTED_BASIC_AUTH).await;

    let owned = post_books_list(
        &app,
        &user_token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
    )
    .await;
    assert_eq!(owned.status(), StatusCode::OK);
    assert_eq!(
        owned
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );
    let owned_json = response_json(owned).await;
    assert_eq!(page_content_ids(&owned_json), vec!["book-oneshot"]);
    assert!(owned_json.get("_compat").is_none());

    let hidden = post_books_list(
        &app,
        &restricted_token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot-restricted"}}"#,
    )
    .await;
    assert_eq!(hidden.status(), StatusCode::OK);
    let hidden_json = response_json(hidden).await;
    assert_eq!(
        hidden_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.visible-single-book)",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn oneshot_bootstrap_rejects_non_oneshot_and_wide_books_list_shapes() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let non_oneshot = post_books_list(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-1"}}"#,
    )
    .await;
    assert_eq!(non_oneshot.status(), StatusCode::OK);
    let non_oneshot_json = response_json(non_oneshot).await;
    assert_eq!(
        non_oneshot_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.series-not-oneshot)",
    );

    let multi_book = post_books_list(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot-multi"}}"#,
    )
    .await;
    assert_eq!(multi_book.status(), StatusCode::OK);
    let multi_book_json = response_json(multi_book).await;
    assert_eq!(
        multi_book_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.visible-single-book)",
    );

    let query_params = post_books_list(
        &app,
        &token,
        "/api/v1/books/list?page=0&size=20",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
    )
    .await;
    assert_eq!(query_params.status(), StatusCode::OK);
    let query_params_json = response_json(query_params).await;
    assert_eq!(
        query_params_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.query-params)",
    );

    let readlist_context = post_books_list(
        &app,
        &token,
        "/api/v1/books/list?context=READLIST&contextId=readlist-2",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
    )
    .await;
    assert_eq!(readlist_context.status(), StatusCode::OK);
    let readlist_context_json = response_json(readlist_context).await;
    assert_eq!(
        readlist_context_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.query-params)",
    );

    let wide_filter = post_books_list(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-oneshot"}]}}"#,
    )
    .await;
    assert_eq!(wide_filter.status(), StatusCode::OK);
    let wide_filter_json = response_json(wide_filter).await;
    assert!(
        wide_filter_json["_compat"]["shape"]
            .as_str()
            .unwrap_or_default()
            .starts_with("UnsupportedBook"),
        "wide books-list shape should stay explicit non-native",
    );
}
