use super::*;

#[tokio::test]
async fn series_list_supported_filters_parity() {
    let app = komga_rust::app::build_router();

    let limited_token =
        session_token_for_basic_auth(&app, "bGltaXRlZEBleGFtcGxlLm9yZzpsaW1pdGVk").await;
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
    let response = series_list_response_for_token(
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
