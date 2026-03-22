use super::*;

pub(super) async fn phase6_readlist_detail_runtime_ownership_is_native() {
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
    );
    let oneshot_bootstrap_json = response_json(oneshot_bootstrap).await;
    assert_eq!(page_content_ids(&oneshot_bootstrap_json), vec!["book-oneshot"]);
    assert!(oneshot_bootstrap_json.get("_compat").is_none());

    let readlist_detail = get_response(&app, &token, "/api/v1/readlists/readlist-1").await;
    assert_eq!(readlist_detail.status(), StatusCode::OK);
    assert_native_owned(&readlist_detail, "readlist detail");
    let readlist_detail_json = response_json(readlist_detail).await;
    assert_eq!(readlist_detail_json["id"], "readlist-1");
    assert_eq!(readlist_detail_json["name"], "ReadList 1");
    assert_eq!(string_array(&readlist_detail_json["bookIds"]), vec!["book-1"]);
    assert_eq!(readlist_detail_json["filtered"], false);
    assert!(readlist_detail_json.get("_compat").is_none());

    let paged_readlist_books = get_response(
        &app,
        &token,
        "/api/v1/readlists/readlist-2/books?page=0&size=20",
    )
    .await;
    assert_eq!(paged_readlist_books.status(), StatusCode::OK);
    assert_shadow_marker(&paged_readlist_books, "paged readlist books");
    let paged_readlist_books_json = response_json(paged_readlist_books).await;
    assert_eq!(
        paged_readlist_books_json["_compat"]["discoveryOwnership"],
        "non-native",
    );
}

pub(super) async fn phase6_readlist_detail_404_and_filtered_semantics_match_contract() {
    let app = komga_rust::app::build_router();
    let restricted_token = session_token_for_basic_auth(&app, RESTRICTED_BASIC_AUTH).await;

    let filtered_readlist = get_response(&app, &restricted_token, "/api/v1/readlists/readlist-2").await;
    assert_eq!(filtered_readlist.status(), StatusCode::OK);
    assert_native_owned(&filtered_readlist, "filtered readlist detail");
    let filtered_json = response_json(filtered_readlist).await;
    assert_eq!(filtered_json["id"], "readlist-2");
    assert_eq!(string_array(&filtered_json["bookIds"]), vec!["book-1"]);
    assert_eq!(filtered_json["filtered"], true);
    assert!(filtered_json.get("_compat").is_none());

    let fully_inaccessible = get_response(&app, &restricted_token, "/api/v1/readlists/readlist-3").await;
    assert_eq!(fully_inaccessible.status(), StatusCode::NOT_FOUND);
    assert_native_owned(&fully_inaccessible, "fully inaccessible readlist detail");

    let missing = get_response(&app, &restricted_token, "/api/v1/readlists/readlist-missing").await;
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
    assert_native_owned(&missing, "missing readlist detail");
}
