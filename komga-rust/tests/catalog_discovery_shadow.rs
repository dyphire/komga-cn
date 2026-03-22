use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::Value;
use tower::util::ServiceExt;

const NATIVE_OWNERSHIP_HEADER: &str = "X-Komga-Compat-Search-Ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";

#[tokio::test]
async fn libraries_admin_user_limited_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let limited_token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;

    let admin_json = libraries_json_for_token(&app, &admin_token).await;
    let user_json = libraries_json_for_token(&app, &user_token).await;
    let limited_json = libraries_json_for_token(&app, &limited_token).await;

    let admin_ids = ids(&admin_json);
    let user_ids = ids(&user_json);
    let limited_ids = ids(&limited_json);

    assert_eq!(admin_ids, vec!["1"]);
    assert_eq!(user_ids, vec!["1"]);
    assert_eq!(limited_ids, vec!["1"]);
}

#[tokio::test]
async fn libraries_non_admin_root_is_sanitized() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let limited_token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;

    let admin_json = libraries_json_for_token(&app, &admin_token).await;
    let user_json = libraries_json_for_token(&app, &user_token).await;
    let limited_json = libraries_json_for_token(&app, &limited_token).await;

    assert_eq!(admin_json[0]["root"], "/library1");

    for library in user_json.as_array().expect("user libraries must be an array") {
        assert_eq!(library["root"], "");
    }
    for library in limited_json
        .as_array()
        .expect("limited libraries must be an array")
    {
        assert_eq!(library["root"], "");
    }
}

#[tokio::test]
async fn series_list_supported_filters_parity() {
    let app = komga_rust::app::build_router();

    let limited_token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
    let json =
        series_list_json_for_token(
            &app,
            &limited_token,
            "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc",
            r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"2"}}"#,
            true,
        )
        .await;

    assert_eq!(json["totalElements"], 0);
    assert_eq!(json["numberOfElements"], 0);
    assert_eq!(json["content"], serde_json::json!([]));
}

#[tokio::test]
async fn series_search_ordering_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let json =
        series_list_json_for_token(
            &app,
            &admin_token,
            "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc",
            r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#,
            true,
        )
        .await;

    assert_eq!(json["totalElements"], 1);
    assert_eq!(json["numberOfElements"], 1);
    assert_eq!(json["sort"]["sorted"], true);
    assert_eq!(json["sort"]["unsorted"], false);
    assert_eq!(json["content"][0]["id"], "series-1");
}

#[tokio::test]
async fn series_unsupported_shape_is_non_native() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response =
        series_list_response_for_token(
            &app,
            &admin_token,
            "/api/v1/series/list?page=0&size=20&sort=random,asc",
            r#"{"fullTextSearch":"series"}"#,
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

#[tokio::test]
async fn series_list_t1_extended_filters_are_native_owned() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let response = series_list_response_for_token(
        &app,
        &admin_token,
        "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc",
        r#"{
            "fullTextSearch":"series",
            "condition":{
                "type":"AllOfSeries",
                "conditions":[
                    {"type":"LibraryId","operator":"is","value":"1"},
                    {"type":"ReadStatus","operator":"is","value":"READ"},
                    {"type":"Genre","operator":"contains","value":"fantasy"},
                    {"type":"Tag","operator":"contains","value":"featured"},
                    {"type":"Language","operator":"is","value":"en"},
                    {"type":"Publisher","operator":"is","value":"komga"},
                    {"type":"AgeRating","operator":"is","value":"16"},
                    {"type":"ReleaseDate","operator":"is","value":"2024-01-01"},
                    {"type":"SharingLabel","operator":"contains","value":"safe"},
                    {"type":"SeriesStatus","operator":"is","value":"ONGOING"},
                    {"type":"Complete","operator":"isTrue"},
                    {"type":"Author","operator":"contains","value":"alice"}
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
    assert_eq!(json["content"][0]["id"], "series-1");
}

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
    assert_ne!(admin_json["content"][0]["url"], user_json["content"][0]["url"]);
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
    assert_ne!(admin_json["content"][0]["url"], user_json["content"][0]["url"]);
}

#[tokio::test]
async fn books_latest_page_metadata_parity() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;

    let paged_json =
        books_latest_json_for_token(&app, &admin_token, "/api/v1/books/latest?page=1&size=1", true)
            .await;
    let unpaged_json =
        books_latest_json_for_token(&app, &admin_token, "/api/v1/books/latest?unpaged=true", true)
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

    let books_latest_response =
        books_latest_response_for_token(&app, &admin_token, "/api/v1/books/latest?page=0&size=20", true)
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

#[tokio::test]
async fn admin_user_limited_restricted_parity_matrix() {
    let app = komga_rust::app::build_router();

    let admin_token = session_token_for_basic_auth(&app, "YWRtaW5AZXhhbXBsZS5vcmc6YWRtaW4=").await;
    let user_token = session_token_for_basic_auth(&app, "dXNlckBleGFtcGxlLm9yZzp1c2Vy").await;
    let limited_token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
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
    let series_body =
        r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let admin_series = series_list_json_for_token(&app, &admin_token, series_path, series_body, true).await;
    let user_series = series_list_json_for_token(&app, &user_token, series_path, series_body, true).await;
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

    let admin_books = books_list_json_for_token(&app, &admin_token, books_path, books_body, true).await;
    let user_books = books_list_json_for_token(&app, &user_token, books_path, books_body, true).await;
    let limited_books = books_list_json_for_token(&app, &limited_token, books_path, books_body, true).await;
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
    let restricted_latest = books_latest_json_for_token(&app, &restricted_token, latest_path, true).await;

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
    let limited_token = session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
    let restricted_token =
        session_token_for_basic_auth(&app, "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk").await;

    let series_path = "/api/v1/series/list?page=0&size=20&sort=metadata.titleSort,asc";
    let limited_library_body =
        r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"2"}}"#;
    let authorized_library_body =
        r#"{"fullTextSearch":"series","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let limited_empty =
        series_list_json_for_token(&app, &limited_token, series_path, limited_library_body, true).await;
    let limited_control =
        series_list_json_for_token(&app, &limited_token, series_path, authorized_library_body, true).await;

    assert_eq!(limited_empty["totalElements"], 0);
    assert_eq!(limited_empty["numberOfElements"], 0);
    assert_eq!(limited_empty["content"], serde_json::json!([]));
    assert_eq!(limited_control["totalElements"], 1);
    assert_eq!(page_content_ids(&limited_control), vec!["series-1"]);

    let books_path = "/api/v1/books/list?page=0&size=20&sort=metadata.title,asc";
    let restricted_only_search = r#"{"fullTextSearch":"restricted-book","condition":{"type":"LibraryId","operator":"is","value":"1"}}"#;

    let restricted_empty =
        books_list_json_for_token(&app, &restricted_token, books_path, restricted_only_search, true).await;
    let admin_control = books_list_json_for_token(&app, &admin_token, books_path, restricted_only_search, true)
        .await;

    assert_eq!(restricted_empty["totalElements"], 0);
    assert_eq!(restricted_empty["numberOfElements"], 0);
    assert_eq!(restricted_empty["content"], serde_json::json!([]));
    assert_eq!(admin_control["totalElements"], 1);
    assert_eq!(book_ids(&admin_control), vec!["book-2"]);
}

fn ids(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .expect("libraries payload should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("library id should be a string")
        })
        .collect()
}

fn book_ids(value: &Value) -> Vec<&str> {
    value
        .get("content")
        .and_then(Value::as_array)
        .expect("books payload content should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("book id should be a string")
        })
        .collect()
}

fn page_content_ids(value: &Value) -> Vec<&str> {
    value
        .get("content")
        .and_then(Value::as_array)
        .expect("page payload content should be an array")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("content id should be a string")
        })
        .collect()
}

async fn session_token_for_basic_auth<S>(app: &S, basic_auth: &str) -> String
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_auth}"))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .expect("login response should include x-auth-token")
        .to_str()
        .expect("x-auth-token should be valid UTF-8")
        .to_string()
}

async fn libraries_json_for_token<S>(app: &S, token: &str) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/libraries")
                .header("X-Auth-Token", token)
                .header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn series_list_json_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = series_list_response_for_token(app, token, path, body, native_owned).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn series_list_response_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("X-Auth-Token", token)
        .header(header::CONTENT_TYPE, "application/json");

    if native_owned {
        request = request.header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
    }

    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn books_list_json_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = books_list_response_for_token(app, token, path, body, native_owned).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn books_list_response_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    body: &str,
    native_owned: bool,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder()
        .method("POST")
        .uri(path)
        .header("X-Auth-Token", token)
        .header(header::CONTENT_TYPE, "application/json");

    if native_owned {
        request = request.header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
    }

    let response = app
        .clone()
        .oneshot(request.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
}

async fn books_latest_json_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    native_owned: bool,
) -> Value
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = books_latest_response_for_token(app, token, path, native_owned).await;
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

async fn books_latest_response_for_token<S>(
    app: &S,
    token: &str,
    path: &str,
    native_owned: bool,
) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let mut request = Request::builder().uri(path).header("X-Auth-Token", token);

    if native_owned {
        request = request.header(NATIVE_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER);
    }

    let response = app
        .clone()
        .oneshot(request.body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
}
