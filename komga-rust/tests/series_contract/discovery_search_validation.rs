use super::*;

#[tokio::test]
async fn router_discovery_series_list_supports_deleted_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list deleted=false request should build"),
        )
        .await
        .expect("strict series/list deleted=false request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    let not_deleted_content = not_deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series deleted=false payload should expose content array");
    assert_eq!(not_deleted_content.len(), 1);

    let deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list deleted=true request should build"),
        )
        .await
        .expect("strict series/list deleted=true request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    let deleted_content = deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series deleted=true payload should expose content array");
    assert_eq!(deleted_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_rejects_legacy_search_query_inputs() {
    let paths = new_router_fixture("router-discovery-series-legacy-search-query").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for legacy_query_name in ["searchRegex", "search_regex"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/v1/series?page=0&size=20&{legacy_query_name}=series"
                    ))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("legacy series query request should build"),
            )
            .await
            .expect("legacy series query request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_legacy_search_query_inputs() {
    let paths =
        new_router_fixture("router-discovery-series-alphabetical-groups-legacy-search-query").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for legacy_query_name in ["regexSearch", "searchRegex", "search_regex"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/api/v1/series/alphabetical-groups?page=0&size=20&{legacy_query_name}=series"
                    ))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("legacy alphabetical-groups query request should build"),
            )
            .await
            .expect("legacy alphabetical-groups query request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-oneshot").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_oneshot_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "OneShot",
                            "operator": "isFalse"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list oneshot=false request should build"),
        )
        .await
        .expect("strict series/list oneshot=false request should complete");
    assert_eq!(not_oneshot_response.status(), StatusCode::OK);
    let not_oneshot_payload = response_json(not_oneshot_response).await;
    let not_oneshot_content = not_oneshot_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series oneshot=false payload should expose content array");
    assert_eq!(not_oneshot_content.len(), 1);

    let oneshot_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "OneShot",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list oneshot=true request should build"),
        )
        .await
        .expect("strict series/list oneshot=true request should complete");
    assert_eq!(oneshot_response.status(), StatusCode::OK);
    let oneshot_payload = response_json(oneshot_response).await;
    let oneshot_content = oneshot_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series oneshot=true payload should expose content array");
    assert_eq!(oneshot_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_rejects_unknown_condition_type_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-unknown-condition").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownSeriesCondition",
                            "operator": "is",
                            "value": "whatever"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unknown-condition request should build"),
        )
        .await
        .expect("strict series/list unknown-condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_rejects_unknown_operator_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-unknown-operator").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Deleted",
                            "operator": "maybe"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unknown-operator request should build"),
        )
        .await
        .expect("strict series/list unknown-operator request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_applies_default_sort_for_unknown_sort_mode_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-series-list-strict-sort-modes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in [
        "metadata.titleSort,asc",
        "createdDate,desc",
        "lastModifiedDate,desc",
        "booksMetadata.releaseDate,desc",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/series/list?page=0&size=20&sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "condition": {
                                "type": "LibraryId",
                                "operator": "is",
                                "value": "library-1"
                            }
                        })
                        .to_string(),
                    ))
                    .expect("strict series/list supported sort request should build"),
            )
            .await
            .expect("strict series/list supported sort request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let unsupported_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20&sort=unsupported.sort,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list unsupported sort request should build"),
        )
        .await
        .expect("strict series/list unsupported sort request should complete");
    assert_eq!(unsupported_response.status(), StatusCode::OK);
    let unsupported_payload = response_json(unsupported_response).await;
    let unsupported_content = unsupported_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series unsupported sort payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);

    cleanup_router_fixture(paths);
}
