use super::*;

async fn soft_delete_series(paths: &RuntimeDbPaths, series_ids: &[&str]) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series deleted fixture db should open");

    for series_id in series_ids {
        sqlx::query("UPDATE SERIES SET DELETED_DATE = ? WHERE ID = ?")
            .bind("2025-01-01 00:00:00")
            .bind(series_id)
            .execute(&pool)
            .await
            .expect("series deleted date should update");
    }

    pool.close().await;
}

fn series_page_ids(payload: &Value) -> Vec<String> {
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("series page payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str))
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn router_discovery_series_get_routes_match_paperback_compatibility_shape() {
    let paths = new_router_fixture("router-discovery-series-papperback-get-compat").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&search_ready_runtime_config_for_paths(&paths));
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    let search_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series?page=0&size=20&search=Series%201&tag=Favorite&genre=SciFi")
                .header(header::AUTHORIZATION, authorization.as_str())
                .body(Body::empty())
                .expect("deprecated series GET request should build"),
        )
        .await
        .expect("deprecated series GET request should complete");

    assert_eq!(search_response.status(), StatusCode::OK);
    let search_payload = response_json(search_response).await;
    assert_eq!(series_page_ids(&search_payload), vec!["series-1"]);

    let detail_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1/")
                .header(header::AUTHORIZATION, authorization.as_str())
                .body(Body::empty())
                .expect("series detail trailing-slash request should build"),
        )
        .await
        .expect("series detail trailing-slash request should complete");

    assert_eq!(detail_response.status(), StatusCode::OK);
    let detail_payload = response_json(detail_response).await;
    assert_eq!(
        detail_payload.get("id"),
        Some(&Value::String("series-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_excludes_soft_deleted_series_by_default() {
    let paths = new_router_fixture("router-discovery-series-list-default-deleted-hidden").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1").await;
    soft_delete_series(&paths, &["series-deleted"]).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
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
                .expect("default series/list request should build"),
        )
        .await
        .expect("default series/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(series_page_ids(&payload), vec!["series-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_deleted_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-series-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-deleted", "Deleted Series", "library-1").await;
    soft_delete_series(&paths, &["series-deleted"]).await;

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
    assert_eq!(series_page_ids(&not_deleted_payload), vec!["series-1"]);

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
    assert_eq!(series_page_ids(&deleted_payload), vec!["series-deleted"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_deleted_filter_handles_deleted_only_library() {
    let paths =
        new_router_fixture("router-discovery-series-list-runtime-only-deleted-visible").await;
    seed_router_contract_data(&paths).await;
    soft_delete_series(&paths, &["series-1"]).await;

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
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("runtime-owned only deleted series/list request should build"),
        )
        .await
        .expect("runtime-owned only deleted series/list request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(series_page_ids(&payload), vec!["series-1"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_removed_series_v1_routes_except_paperback_compatibility_return_not_found()
{
    let paths = new_router_fixture("router-discovery-removed-v1-series-routes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/series/alphabetical-groups?page=0&size=20",
        "/api/v1/series/series-1/books?page=0&size=20",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("removed series v1 request should build"),
            )
            .await
            .expect("removed series v1 request should complete");

        assert_eq!(response.status(), StatusCode::NOT_FOUND, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_groups_by_title_sort_first_character() {
    let paths = new_router_fixture("router-discovery-series-alphabetical-groups-title-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;
    seed_router_custom_series(&paths, "series-3", "Series 3", "library-1").await;
    seed_router_series_title_sort(&paths, "series-1", "Alpha Shelf").await;
    seed_router_series_title_sort(&paths, "series-2", "Beta Shelf").await;
    seed_router_series_title_sort(&paths, "series-3", "Beta Archive").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list/alphabetical-groups")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .expect("alphabetical-groups request should build"),
        )
        .await
        .expect("alphabetical-groups request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let groups = payload
        .as_array()
        .expect("alphabetical-groups payload should be an array")
        .iter()
        .map(|entry| {
            (
                entry
                    .get("group")
                    .and_then(Value::as_str)
                    .expect("group entry should expose group")
                    .to_string(),
                entry
                    .get("count")
                    .and_then(Value::as_i64)
                    .expect("group entry should expose count"),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(groups, vec![("a".to_string(), 1), ("b".to_string(), 2)]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_unknown_condition_type() {
    let paths =
        new_router_fixture("router-discovery-series-alphabetical-groups-unknown-condition").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list/alphabetical-groups")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownSeriesCondition",
                            "operator": "isTrue"
                        }
                    })
                    .to_string(),
                ))
                .expect("invalid alphabetical-groups request should build"),
        )
        .await
        .expect("invalid alphabetical-groups request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload,
        json!({
            "error": "invalid series alphabetical-groups request: InvalidSemantics(\"unsupported series condition type: UnknownSeriesCondition\")"
        })
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_empty_untyped_condition() {
    let paths =
        new_router_fixture("router-discovery-series-alphabetical-groups-empty-condition").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (case, body) in [
        ("empty-condition", json!({ "condition": {} })),
        (
            "unknown-webui-leaf",
            json!({
                "condition": {
                    "unknownField": {
                        "operator": "isTrue"
                    }
                }
            }),
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list/alphabetical-groups")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("alphabetical-groups invalid condition request should build"),
            )
            .await
            .expect("alphabetical-groups invalid condition request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "case: {case}");
        let payload = response_json(response).await;
        let error = payload
            .get("error")
            .and_then(Value::as_str)
            .expect("invalid condition response should expose error string");
        assert!(
            error.starts_with("invalid series alphabetical-groups request:"),
            "case: {case}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_alphabetical_groups_rejects_non_object_bodies() {
    let paths =
        new_router_fixture("router-discovery-series-alphabetical-groups-non-object-body").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (case, body) in [("array", Body::from("[]")), ("null", Body::from("null"))] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list/alphabetical-groups")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body)
                    .expect("non-object alphabetical-groups request should build"),
            )
            .await
            .expect("non-object alphabetical-groups request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST, "case: {case}");
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
        "created,desc",
        "lastModifiedDate,desc",
        "lastModified,desc",
        "booksMetadata.releaseDate,desc",
        "booksCount,desc",
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

#[tokio::test]
async fn router_discovery_series_list_sorts_runtime_owned_results_by_release_date_books_count_and_alias_dates()
 {
    let paths = new_router_fixture("router-discovery-series-list-runtime-sort-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_custom_series(&paths, "series-2", "Series 2", "library-1").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("series runtime sort db should open");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Zulu Series")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series-1 title sort should update");
    sqlx::query("UPDATE SERIES_METADATA SET TITLE_SORT = ? WHERE SERIES_ID = ?")
        .bind("Alpha Series")
        .bind("series-2")
        .execute(&pool)
        .await
        .expect("series-2 title sort should update");
    for (series_id, created, last_modified, book_count) in [
        (
            "series-1",
            "2024-01-01 00:00:00",
            "2024-01-10 00:00:00",
            1_i64,
        ),
        (
            "series-2",
            "2024-02-01 00:00:00",
            "2024-02-10 00:00:00",
            3_i64,
        ),
    ] {
        sqlx::query(
            "UPDATE SERIES \
             SET CREATED_DATE = ?, LAST_MODIFIED_DATE = ?, BOOK_COUNT = ? \
             WHERE ID = ?",
        )
        .bind(created)
        .bind(last_modified)
        .bind(book_count)
        .bind(series_id)
        .execute(&pool)
        .await
        .expect("series runtime sort fixture should update series timestamps and counts");
    }
    sqlx::query(
        "UPDATE BOOK_METADATA_AGGREGATION \
         SET RELEASE_DATE = ? \
         WHERE SERIES_ID = ?",
    )
    .bind("2024-01-15")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series-1 aggregation release date should update");
    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("2024-02-15")
    .bind("")
    .bind("")
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("series-2 aggregation release date should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (sort, expected_ids) in [
        ("metadata.titleSort,asc", vec!["series-2", "series-1"]),
        ("titleSort,asc", vec!["series-2", "series-1"]),
        ("created,desc", vec!["series-2", "series-1"]),
        ("lastModified,desc", vec!["series-2", "series-1"]),
        (
            "booksMetadata.releaseDate,desc",
            vec!["series-2", "series-1"],
        ),
        ("booksCount,desc", vec!["series-2", "series-1"]),
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
                    .expect("series runtime sort request should build"),
            )
            .await
            .expect("series runtime sort request should complete");
        assert_eq!(response.status(), StatusCode::OK, "sort: {sort}");
        let payload = response_json(response).await;
        let ids = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("series runtime sort payload should expose content array")
            .iter()
            .filter_map(|entry| entry.get("id").and_then(Value::as_str))
            .collect::<Vec<_>>();
        assert_eq!(ids, expected_ids, "sort: {sort}");
    }

    cleanup_router_fixture(paths);
}
