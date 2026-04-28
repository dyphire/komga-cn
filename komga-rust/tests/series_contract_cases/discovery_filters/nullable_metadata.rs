use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_nullable_metadata_operators_with_null_rows_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-nullable-metadata-positive").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths)).await;
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (condition_type, operator, expected_id) in [
        ("Tag", "is", "series-1"),
        ("Tag", "isNot", "series-2"),
        ("Tag", "isNull", "series-2"),
        ("Tag", "isNotNull", "series-1"),
        ("Genre", "is", "series-1"),
        ("Genre", "isNot", "series-2"),
        ("Genre", "isNull", "series-2"),
        ("Genre", "isNotNull", "series-1"),
        ("SharingLabel", "is", "series-1"),
        ("SharingLabel", "isNot", "series-2"),
        ("SharingLabel", "isNull", "series-2"),
        ("SharingLabel", "isNotNull", "series-1"),
    ] {
        let value = match condition_type {
            "Tag" => "Favorite",
            "Genre" => "SciFi",
            _ => "Family",
        };
        let body = if operator == "is" || operator == "isNot" {
            json!({
                "condition": {
                    "type": condition_type,
                    "operator": operator,
                    "value": value,
                }
            })
            .to_string()
        } else {
            json!({
                "condition": {
                    "type": condition_type,
                    "operator": operator,
                }
            })
            .to_string()
        };

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("strict series/list nullable metadata request should build"),
            )
            .await
            .expect("strict series/list nullable metadata request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series nullable metadata payload should expose content array");
        assert_eq!(
            content.len(),
            1,
            "unexpected series nullable metadata count for type={condition_type}, operator={operator}",
        );
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String(expected_id.to_string())),
            "unexpected series nullable metadata id for type={condition_type}, operator={operator}",
        );
    }

    cleanup_router_fixture(paths);
}
