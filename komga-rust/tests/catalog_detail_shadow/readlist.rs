use super::*;

#[test]
fn readlist_books_paged_variant_remains_non_native() {
    let queries = DiscoveryQueries::new(SqliteDiscoveryAdapter::default());
    let context = DiscoveryQueryContext::allow_all();

    let result = queries.list_readlist_books(
        &context,
        ReadListBooksQuery {
            readlist_id: "readlist-1".to_string(),
            page: 0,
            size: 20,
            unpaged: false,
            library_ids: None,
        },
    );

    assert!(matches!(
        result,
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));
}

#[test]
fn readlist_books_library_id_variant_remains_non_native() {
    let queries = DiscoveryQueries::new(SqliteDiscoveryAdapter::default());
    let context = DiscoveryQueryContext::allow_all();

    let result = queries.list_readlist_books(
        &context,
        ReadListBooksQuery {
            readlist_id: "readlist-1".to_string(),
            page: 0,
            size: 20,
            unpaged: true,
            library_ids: Some(vec!["1".to_string()]),
        },
    );

    assert!(matches!(
        result,
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));
}

#[tokio::test]
async fn readlist_books_runtime_ownership_stays_narrow() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let previous_response = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books/book-1/previous",
    )
    .await;
    assert_eq!(previous_response.status(), StatusCode::NOT_FOUND);
    assert_native_owned(&previous_response, "readlist previous boundary");

    let next_response = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books/book-1/next",
    )
    .await;
    assert_eq!(next_response.status(), StatusCode::OK);
    assert_native_owned(&next_response, "readlist next sibling");
    let next_json = response_json(next_response).await;
    assert_eq!(next_json["id"], "book-2");
    assert!(next_json.get("_compat").is_none());

    let native_response = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books?unpaged=true",
    )
    .await;
    assert_eq!(native_response.status(), StatusCode::OK);
    assert_eq!(
        native_response
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
        "bare unpaged readlist books route should be native-owned",
    );
    let native_json = response_json(native_response).await;
    assert_eq!(page_content_ids(&native_json), vec!["book-1", "book-2"]);
    assert_eq!(native_json["pageable"]["paged"], false);
    assert_eq!(native_json["pageable"]["unpaged"], true);
    assert!(native_json.get("_compat").is_none());

    let paged_response = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books?page=0&size=20",
    )
    .await;
    assert_eq!(paged_response.status(), StatusCode::OK);
    assert_shadow_marker(&paged_response, "paged readlist books");
    let paged_json = response_json(paged_response).await;
    assert_eq!(paged_json["_compat"]["discoveryOwnership"], "non-native");
    assert_eq!(
        paged_json["_compat"]["shape"],
        "UnsupportedBookFilter(paged)"
    );

    let library_scoped_response = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books?unpaged=true&library_id=1",
    )
    .await;
    assert_eq!(library_scoped_response.status(), StatusCode::OK);
    assert_shadow_marker(&library_scoped_response, "library-scoped readlist books");
    let library_scoped_json = response_json(library_scoped_response).await;
    assert_eq!(
        library_scoped_json["_compat"]["discoveryOwnership"],
        "non-native"
    );
    assert_eq!(
        library_scoped_json["_compat"]["shape"],
        "UnsupportedBookFilter(LibraryId)",
    );
}
