use super::*;

fn assert_spring_bad_request(payload: &Value, message: &str, path: &str) {
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert_eq!(
        payload.get("message"),
        Some(&Value::String(message.to_string()))
    );
    assert_eq!(payload.get("status"), Some(&Value::from(400)));
    assert_eq!(payload.get("path"), Some(&Value::String(path.to_string())));
    assert!(
        payload.get("timestamp").and_then(Value::as_u64).is_some(),
        "expected numeric timestamp in spring-style error payload: {payload:?}"
    );
}

#[tokio::test]
async fn router_readlist_match_comicrack_rejects_invalid_xml_and_reports_matches() {
    let paths = new_router_fixture("router-readlist-match-comicrack").await;
    seed_router_contract_data(&paths).await;
    seed_readlist_endpoint_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let missing_file_part_body = format!(
        "--komga-rust-comicrack-boundary\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"list.cbl\"\r\nContent-Type: application/xml\r\n\r\n<ReadingList />\r\n--komga-rust-comicrack-boundary--\r\n"
    );
    let missing_file_part = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(
                    header::CONTENT_TYPE,
                    "multipart/form-data; boundary=komga-rust-comicrack-boundary",
                )
                .body(Body::from(missing_file_part_body))
                .expect("missing-file-part comicrack request should build"),
        )
        .await
        .expect("missing-file-part comicrack request should complete");
    assert_eq!(missing_file_part.status(), StatusCode::BAD_REQUEST);
    let missing_file_part_payload = response_json(missing_file_part).await;
    assert_spring_bad_request(
        &missing_file_part_payload,
        "Required request part 'file' is not present",
        "/api/v1/readlists/match/comicrack",
    );

    let malformed_multipart = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "multipart/form-data")
                .body(Body::from("garbled multipart body"))
                .expect("malformed comicrack request should build"),
        )
        .await
        .expect("malformed comicrack request should complete");
    assert_eq!(malformed_multipart.status(), StatusCode::BAD_REQUEST);
    let malformed_multipart_payload = response_json(malformed_multipart).await;
    assert_eq!(
        malformed_multipart_payload.get("error"),
        Some(&Value::String("Bad Request".to_string()))
    );
    assert!(
        malformed_multipart_payload
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|message| message.to_ascii_lowercase().contains("boundary")),
        "malformed multipart should expose boundary-style binding failure, got: {malformed_multipart_payload:?}"
    );

    let (invalid_content_type, invalid_body) = comicrack_multipart_body("<ReadingList>");
    let invalid_xml = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, invalid_content_type)
                .body(Body::from(invalid_body))
                .expect("invalid comicrack request should build"),
        )
        .await
        .expect("invalid comicrack request should complete");
    assert_eq!(invalid_xml.status(), StatusCode::BAD_REQUEST);
    let invalid_payload = response_json(invalid_xml).await;
    assert_spring_bad_request(
        &invalid_payload,
        "ERR_1015",
        "/api/v1/readlists/match/comicrack",
    );

    let (missing_books_content_type, missing_books_body) = comicrack_multipart_body(
        r#"<?xml version="1.0"?><ReadingList><Name>RL</Name><Books></Books></ReadingList>"#,
    );
    let missing_books = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, missing_books_content_type)
                .body(Body::from(missing_books_body))
                .expect("missing-books comicrack request should build"),
        )
        .await
        .expect("missing-books comicrack request should complete");
    assert_eq!(missing_books.status(), StatusCode::BAD_REQUEST);
    let missing_books_payload = response_json(missing_books).await;
    assert_spring_bad_request(
        &missing_books_payload,
        "ERR_1029",
        "/api/v1/readlists/match/comicrack",
    );

    let xml = r#"<ReadingList><Name>ReadList 1</Name><Books><Book Series="Series 2" Number="002" /></Books></ReadingList>"#;
    let (valid_content_type, valid_body) = comicrack_multipart_body_with_quoted_boundary(xml);
    let valid = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/match/comicrack")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, valid_content_type)
                .body(Body::from(valid_body))
                .expect("valid comicrack request should build"),
        )
        .await
        .expect("valid comicrack request should complete");
    assert_eq!(valid.status(), StatusCode::OK);
    let valid_payload = response_json(valid).await;
    assert_eq!(
        valid_payload
            .get("readListMatch")
            .and_then(|it| it.get("name"))
            .and_then(Value::as_str),
        Some("ReadList 1"),
    );
    assert_eq!(
        valid_payload
            .get("readListMatch")
            .and_then(|it| it.get("errorCode"))
            .and_then(Value::as_str),
        Some("ERR_1009"),
    );
    let requests = valid_payload
        .get("requests")
        .and_then(Value::as_array)
        .expect("valid comicrack payload should expose requests array");
    assert_eq!(requests.len(), 1);
    let matches = requests[0]
        .get("matches")
        .and_then(Value::as_array)
        .expect("valid comicrack request should expose matches array");
    assert_eq!(matches.len(), 1);
    assert_eq!(
        matches[0]
            .get("series")
            .and_then(|series| series.get("releaseDate"))
            .and_then(Value::as_str),
        Some("2024-01-01"),
    );
    assert_eq!(
        matches[0]
            .get("books")
            .and_then(Value::as_array)
            .and_then(|books| books.first())
            .and_then(|book| book.get("bookId"))
            .and_then(Value::as_str),
        Some("book-2"),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_series_list_supports_metadata_and_collection_filters_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-series-list-strict-metadata-and-collection").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let genre_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Genre", "operator": "contains", "value": "Sci"}})
                        .to_string(),
                ))
                .expect("strict series/list genre match request should build"),
        )
        .await
        .expect("strict series/list genre match request should complete");
    assert_eq!(genre_match.status(), StatusCode::OK);
    let genre_match_payload = response_json(genre_match).await;
    let genre_match_content = genre_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series genre match payload should expose content array");
    assert_eq!(genre_match_content.len(), 1);

    let genre_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Genre", "operator": "contains", "value": "Drama"}})
                        .to_string(),
                ))
                .expect("strict series/list genre miss request should build"),
        )
        .await
        .expect("strict series/list genre miss request should complete");
    assert_eq!(genre_miss.status(), StatusCode::OK);
    let genre_miss_payload = response_json(genre_miss).await;
    let genre_miss_content = genre_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series genre miss payload should expose content array");
    assert_eq!(genre_miss_content.len(), 0);

    let collection_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "CollectionId", "operator": "is", "value": "collection-1"}})
                        .to_string(),
                ))
                .expect("strict series/list collection match request should build"),
        )
        .await
        .expect("strict series/list collection match request should complete");
    assert_eq!(collection_match.status(), StatusCode::OK);
    let collection_match_payload = response_json(collection_match).await;
    let collection_match_content = collection_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series collection match payload should expose content array");
    assert_eq!(collection_match_content.len(), 1);

    let collection_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "CollectionId", "operator": "is", "value": "collection-missing"}})
                        .to_string(),
                ))
                .expect("strict series/list collection miss request should build"),
        )
        .await
        .expect("strict series/list collection miss request should complete");
    assert_eq!(collection_miss.status(), StatusCode::OK);
    let collection_miss_payload = response_json(collection_miss).await;
    let collection_miss_content = collection_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series collection miss payload should expose content array");
    assert_eq!(collection_miss_content.len(), 0);

    let language_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "is", "value": "en"}})
                        .to_string(),
                ))
                .expect("strict series/list language match request should build"),
        )
        .await
        .expect("strict series/list language match request should complete");
    assert_eq!(language_match.status(), StatusCode::OK);
    let language_match_payload = response_json(language_match).await;
    let language_match_content = language_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language match payload should expose content array");
    assert_eq!(language_match_content.len(), 1);

    let language_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "isNot", "value": "en"}})
                        .to_string(),
                ))
                .expect("strict series/list language isNot excluded request should build"),
        )
        .await
        .expect("strict series/list language isNot excluded request should complete");
    assert_eq!(language_is_not_excluded.status(), StatusCode::OK);
    let language_is_not_excluded_payload = response_json(language_is_not_excluded).await;
    let language_is_not_excluded_content = language_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language isNot excluded payload should expose content array");
    assert_eq!(language_is_not_excluded_content.len(), 0);

    let language_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Language", "operator": "isNot", "value": "fr"}})
                        .to_string(),
                ))
                .expect("strict series/list language isNot kept request should build"),
        )
        .await
        .expect("strict series/list language isNot kept request should complete");
    assert_eq!(language_is_not_kept.status(), StatusCode::OK);
    let language_is_not_kept_payload = response_json(language_is_not_kept).await;
    let language_is_not_kept_content = language_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series language isNot kept payload should expose content array");
    assert_eq!(language_is_not_kept_content.len(), 1);

    let publisher_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "is", "value": "PubHouse"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher match request should build"),
        )
        .await
        .expect("strict series/list publisher match request should complete");
    assert_eq!(publisher_match.status(), StatusCode::OK);
    let publisher_match_payload = response_json(publisher_match).await;
    let publisher_match_content = publisher_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher match payload should expose content array");
    assert_eq!(publisher_match_content.len(), 1);

    let publisher_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "isNot", "value": "PubHouse"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher isNot excluded request should build"),
        )
        .await
        .expect("strict series/list publisher isNot excluded request should complete");
    assert_eq!(publisher_is_not_excluded.status(), StatusCode::OK);
    let publisher_is_not_excluded_payload = response_json(publisher_is_not_excluded).await;
    let publisher_is_not_excluded_content = publisher_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher isNot excluded payload should expose content array");
    assert_eq!(publisher_is_not_excluded_content.len(), 0);

    let publisher_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Publisher", "operator": "isNot", "value": "OtherPub"}})
                        .to_string(),
                ))
                .expect("strict series/list publisher isNot kept request should build"),
        )
        .await
        .expect("strict series/list publisher isNot kept request should complete");
    assert_eq!(publisher_is_not_kept.status(), StatusCode::OK);
    let publisher_is_not_kept_payload = response_json(publisher_is_not_kept).await;
    let publisher_is_not_kept_content = publisher_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series publisher isNot kept payload should expose content array");
    assert_eq!(publisher_is_not_kept_content.len(), 1);

    let age_rating_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "AgeRating", "operator": "is", "value": 16}})
                        .to_string(),
                ))
                .expect("strict series/list age-rating match request should build"),
        )
        .await
        .expect("strict series/list age-rating match request should complete");
    assert_eq!(age_rating_match.status(), StatusCode::OK);
    let age_rating_match_payload = response_json(age_rating_match).await;
    let age_rating_match_content = age_rating_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series age-rating match payload should expose content array");
    assert_eq!(age_rating_match_content.len(), 1);

    for (operator, body, expected_count) in [
        (
            "isNot",
            json!({"condition": {"type": "AgeRating", "operator": "isNot", "value": 16}}),
            0_usize,
        ),
        (
            "isNot",
            json!({"condition": {"type": "AgeRating", "operator": "isNot", "value": 18}}),
            1_usize,
        ),
        (
            "greaterThan",
            json!({"condition": {"type": "AgeRating", "operator": "greaterThan", "value": 15}}),
            1_usize,
        ),
        (
            "greaterThan",
            json!({"condition": {"type": "AgeRating", "operator": "greaterThan", "value": 16}}),
            0_usize,
        ),
        (
            "lessThan",
            json!({"condition": {"type": "AgeRating", "operator": "lessThan", "value": 17}}),
            1_usize,
        ),
        (
            "lessThan",
            json!({"condition": {"type": "AgeRating", "operator": "lessThan", "value": 16}}),
            0_usize,
        ),
        (
            "isNull",
            json!({"condition": {"type": "AgeRating", "operator": "isNull"}}),
            0_usize,
        ),
        (
            "isNotNull",
            json!({"condition": {"type": "AgeRating", "operator": "isNotNull"}}),
            1_usize,
        ),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("strict series/list age-rating operator request should build"),
            )
            .await
            .expect("strict series/list age-rating operator request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series age-rating operator payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected strict series age-rating count for operator={operator}",
        );
    }

    let sharing_label_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "SharingLabel", "operator": "contains", "value": "fam"}})
                        .to_string(),
                ))
                .expect("strict series/list sharing-label match request should build"),
        )
        .await
        .expect("strict series/list sharing-label match request should complete");
    assert_eq!(sharing_label_match.status(), StatusCode::OK);
    let sharing_label_match_payload = response_json(sharing_label_match).await;
    let sharing_label_match_content = sharing_label_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series sharing-label match payload should expose content array");
    assert_eq!(sharing_label_match_content.len(), 1);

    for (condition_type, operator, expected_count) in [
        ("Tag", "isNot", 0_usize),
        ("Tag", "isNull", 0_usize),
        ("Tag", "isNotNull", 1_usize),
        ("Genre", "isNot", 0_usize),
        ("Genre", "isNull", 0_usize),
        ("Genre", "isNotNull", 1_usize),
        ("SharingLabel", "isNot", 0_usize),
        ("SharingLabel", "isNull", 0_usize),
        ("SharingLabel", "isNotNull", 1_usize),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/series/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(if operator == "isNot" {
                        json!({
                            "condition": {
                                "type": condition_type,
                                "operator": operator,
                                "value": match condition_type {
                                    "Tag" => "Favorite",
                                    "Genre" => "SciFi",
                                    _ => "Family",
                                }
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
                    }))
                    .expect("strict series/list nullable string-op request should build"),
            )
            .await
            .expect("strict series/list nullable string-op request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series nullable string-op payload should expose content array");
        assert_eq!(
            content.len(),
            expected_count,
            "unexpected series nullable result for type={condition_type}, operator={operator}",
        );
    }

    let author_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "john"}})
                        .to_string(),
                ))
                .expect("strict series/list author match request should build"),
        )
        .await
        .expect("strict series/list author match request should complete");
    assert_eq!(author_match.status(), StatusCode::OK);
    let author_match_payload = response_json(author_match).await;
    let author_match_content = author_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author match payload should expose content array");
    assert_eq!(author_match_content.len(), 1);

    let author_role_match = app
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
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "John Doe",
                                "role": "writer"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list author role match request should build"),
        )
        .await
        .expect("strict series/list author role match request should complete");
    assert_eq!(author_role_match.status(), StatusCode::OK);
    let author_role_match_payload = response_json(author_role_match).await;
    let author_role_match_content = author_role_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author role match payload should expose content array");
    assert_eq!(author_role_match_content.len(), 1);

    let author_role_miss = app
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
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "John Doe",
                                "role": "editor"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict series/list author role miss request should build"),
        )
        .await
        .expect("strict series/list author role miss request should complete");
    assert_eq!(author_role_miss.status(), StatusCode::OK);
    let author_role_miss_payload = response_json(author_role_miss).await;
    let author_role_miss_content = author_role_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict series author role miss payload should expose content array");
    assert_eq!(author_role_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}
