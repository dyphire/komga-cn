use super::*;

#[tokio::test]
async fn books_list_supported_filters_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let json = books_list_json_for_token(
        &app,
        &admin_token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        r#"{"fullTextSearch":"book","condition":{"type":"AllOfBook","conditions":[{"type":"LibraryId","operator":"is","value":"1"},{"type":"SeriesId","operator":"is","value":"series-1"}]}}"#,
        true,
    )
    .await;

    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["numberOfElements"], 1);
    assert_eq!(json["content"][0]["id"], "book-1");
}

#[tokio::test]
async fn books_list_t1_extended_filters_are_native_owned() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = books_list_response_for_token(
        &app,
        &admin_token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        r#"{
            "fullTextSearch":"book",
            "condition":{
                "type":"AllOfBook",
                "conditions":[
                    {"type":"LibraryId","operator":"is","value":"1"},
                    {"type":"SeriesId","operator":"is","value":"series-1"},
                    {"type":"ReadStatus","operator":"is","value":"READ"},
                    {"type":"MediaProfile","operator":"is","value":"PROFILE-1"},
                    {"type":"MediaStatus","operator":"is","value":"READY"},
                    {"type":"Author","operator":"contains","value":"alice"},
                    {"type":"ReleaseDate","operator":"is","value":"2024-01-01"}
                ]
            }
        }"#,
        true,
    )
    .await;

    assert_eq!(
        response
            .headers()
            .get(NATIVE_OWNERSHIP_HEADER)
            .and_then(|value| value.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["content"][0]["id"], "book-1");
}

#[tokio::test]
async fn books_non_admin_url_is_restricted() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;

    let path = "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc";
    let body =
        r#"{"fullTextSearch":"book","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let admin_json = books_list_json_for_token(&app, &admin_token, path, body, true).await;
    let user_json = books_list_json_for_token(&app, &user_token, path, body, true).await;

    assert_eq!(user_json["content"][0]["url"], "book.cbr");
    assert_ne!(
        admin_json["content"][0]["url"],
        user_json["content"][0]["url"]
    );
}

#[tokio::test]
async fn books_restricted_content_is_filtered() {
    let app = komga_rust::app::build_router();

    let restricted_token =
        session_token_for_basic_auth(&app, "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk").await;
    let json = books_list_json_for_token(
        &app,
        &restricted_token,
        "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc",
        r#"{"condition":{"type":"LibraryId","operator":"is","value":"1"}}"#,
        true,
    )
    .await;

    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["content"][0]["id"], "book-1");
}

#[tokio::test]
async fn books_latest_descending_order_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let restricted_token =
        session_token_for_basic_auth(&app, "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk").await;

    let path = "/api/v1/books/latest?page=0&size=20";
    let admin_json = books_latest_json_for_token(&app, &admin_token, path, true).await;
    let user_json = books_latest_json_for_token(&app, &user_token, path, true).await;
    let restricted_json = books_latest_json_for_token(&app, &restricted_token, path, true).await;

    assert_eq!(book_ids(&admin_json), vec!["book-2", "book-1"]);
    assert_eq!(book_ids(&user_json), vec!["book-2", "book-1"]);
    assert_eq!(book_ids(&restricted_json), vec!["book-1"]);
    assert_eq!(user_json["content"][0]["url"], "restricted-book.cbz");
    assert_ne!(
        admin_json["content"][0]["url"],
        user_json["content"][0]["url"]
    );
}

#[tokio::test]
async fn books_latest_page_metadata_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;

    let paged_json = books_latest_json_for_token(
        &app,
        &admin_token,
        "/api/v1/books/latest?page=1&size=1",
        true,
    )
    .await;
    let unpaged_json = books_latest_json_for_token(
        &app,
        &admin_token,
        "/api/v1/books/latest?unpaged=true",
        true,
    )
    .await;

    assert_eq!(paged_json["number"], 1);
    assert_eq!(paged_json["size"], 1);
    assert_eq!(paged_json["numberOfElements"], 1);
    assert_eq!(paged_json["totalElements"], 2);
    assert_eq!(paged_json["totalPages"], 2);
    assert_eq!(paged_json["first"], false);
    assert_eq!(paged_json["last"], true);
    assert_eq!(paged_json["content"][0]["id"], "book-1");
    assert_eq!(paged_json["sort"]["sorted"], true);
    assert_eq!(paged_json["pageable"]["pageNumber"], 1);
    assert_eq!(paged_json["pageable"]["pageSize"], 1);
    assert_eq!(paged_json["pageable"]["offset"], 1);
    assert_eq!(paged_json["pageable"]["paged"], true);
    assert_eq!(paged_json["pageable"]["unpaged"], false);

    assert_eq!(book_ids(&unpaged_json), vec!["book-2", "book-1"]);
    assert_eq!(unpaged_json["totalElements"], 2);
    assert_eq!(unpaged_json["numberOfElements"], 2);
    assert_eq!(unpaged_json["first"], true);
    assert_eq!(unpaged_json["last"], true);
    assert_eq!(unpaged_json["pageable"]["paged"], false);
    assert_eq!(unpaged_json["pageable"]["unpaged"], true);
}

#[tokio::test]
async fn books_latest_sort_override_is_non_native() {
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
}
