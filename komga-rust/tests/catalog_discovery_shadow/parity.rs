use super::*;

#[tokio::test]
async fn admin_user_limited_restricted_parity_matrix() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let limited_token =
        session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
    let restricted_token =
        session_token_for_basic_auth(&app, "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk").await;

    let admin_libraries_json = libraries_json_for_token(&app, &admin_token).await;
    let user_libraries_json = libraries_json_for_token(&app, &user_token).await;
    let limited_libraries_json = libraries_json_for_token(&app, &limited_token).await;
    let restricted_libraries_json = libraries_json_for_token(&app, &restricted_token).await;

    let admin_libraries = ids(&admin_libraries_json);
    let user_libraries = ids(&user_libraries_json);
    let limited_libraries = ids(&limited_libraries_json);
    let restricted_libraries = ids(&restricted_libraries_json);

    assert_eq!(admin_libraries, vec!["1"]);
    assert_eq!(user_libraries, admin_libraries);
    assert_eq!(limited_libraries, admin_libraries);
    assert_eq!(restricted_libraries, admin_libraries);

    let series_path = "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc";
    let series_body = r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let admin_series =
        series_list_json_for_token(&app, &admin_token, series_path, series_body, true).await;
    let user_series =
        series_list_json_for_token(&app, &user_token, series_path, series_body, true).await;
    let limited_series =
        series_list_json_for_token(&app, &limited_token, series_path, series_body, true).await;
    let restricted_series =
        series_list_json_for_token(&app, &restricted_token, series_path, series_body, true).await;

    let admin_series_ids = page_content_ids(&admin_series);
    assert_eq!(admin_series_ids, vec!["series-1"]);
    assert_eq!(page_content_ids(&user_series), admin_series_ids);
    assert_eq!(page_content_ids(&limited_series), admin_series_ids);
    assert_eq!(page_content_ids(&restricted_series), admin_series_ids);

    let books_path = "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc";
    let books_body = r#"{"condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let admin_books =
        books_list_json_for_token(&app, &admin_token, books_path, books_body, true).await;
    let user_books =
        books_list_json_for_token(&app, &user_token, books_path, books_body, true).await;
    let limited_books =
        books_list_json_for_token(&app, &limited_token, books_path, books_body, true).await;
    let restricted_books =
        books_list_json_for_token(&app, &restricted_token, books_path, books_body, true).await;

    let admin_books_ids = book_ids(&admin_books);
    assert_eq!(admin_books_ids, vec!["book-1", "book-2"]);
    assert_eq!(book_ids(&user_books), admin_books_ids);
    assert_eq!(book_ids(&limited_books), admin_books_ids);
    assert_eq!(book_ids(&restricted_books), vec!["book-1"]);

    let latest_path = "/api/v1/books/latest?page=0&size=20";
    let admin_latest = books_latest_json_for_token(&app, &admin_token, latest_path, true).await;
    let user_latest = books_latest_json_for_token(&app, &user_token, latest_path, true).await;
    let limited_latest = books_latest_json_for_token(&app, &limited_token, latest_path, true).await;
    let restricted_latest =
        books_latest_json_for_token(&app, &restricted_token, latest_path, true).await;

    let admin_latest_ids = book_ids(&admin_latest);
    assert_eq!(admin_latest_ids, vec!["book-2", "book-1"]);
    assert_eq!(book_ids(&user_latest), admin_latest_ids);
    assert_eq!(book_ids(&limited_latest), admin_latest_ids);
    assert_eq!(book_ids(&restricted_latest), vec!["book-1"]);
}

#[tokio::test]
async fn discovery_empty_result_scenarios_are_explicit() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let limited_token =
        session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
    let restricted_token =
        session_token_for_basic_auth(&app, "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk").await;

    let series_path = "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc";
    let limited_library_body = r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"2"}}"#;
    let authorized_library_body = r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let limited_empty = series_list_json_for_token(
        &app,
        &limited_token,
        series_path,
        limited_library_body,
        true,
    )
    .await;
    let limited_control = series_list_json_for_token(
        &app,
        &limited_token,
        series_path,
        authorized_library_body,
        true,
    )
    .await;

    assert_eq!(limited_empty["totalElements"], 0);
    assert_eq!(limited_empty["numberOfElements"], 0);
    assert_eq!(limited_empty["content"], serde_json::json!([]));
    assert_eq!(limited_control["totalElements"], 1);
    assert_eq!(page_content_ids(&limited_control), vec!["series-1"]);

    let books_path = "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc";
    let restricted_only_search = r#"{"fullTextSearch":"restricted-book","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let restricted_empty = books_list_json_for_token(
        &app,
        &restricted_token,
        books_path,
        restricted_only_search,
        true,
    )
    .await;
    let admin_control =
        books_list_json_for_token(&app, &admin_token, books_path, restricted_only_search, true)
            .await;

    assert_eq!(restricted_empty["totalElements"], 0);
    assert_eq!(restricted_empty["numberOfElements"], 0);
    assert_eq!(restricted_empty["content"], serde_json::json!([]));
    assert_eq!(admin_control["totalElements"], 1);
    assert_eq!(book_ids(&admin_control), vec!["book-2"]);
}
