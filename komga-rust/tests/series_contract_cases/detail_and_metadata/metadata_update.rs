use super::*;

#[tokio::test]
async fn router_discovery_series_metadata_update_refreshes_series_last_modified() {
    let ctx = TestFixture::new("router-discovery-series-metadata-refresh").await;
    let auth_token = ctx.login_admin().await;

    let before_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail before metadata update request should build"),
        )
        .await
        .expect("series detail before metadata update request should complete");
    assert_eq!(before_response.status(), StatusCode::OK);
    let before_payload = response_json(before_response).await;
    let before_last_modified = before_payload
        .get("lastModified")
        .and_then(Value::as_str)
        .expect("series detail payload should expose lastModified")
        .to_string();

    sleep(Duration::from_millis(1100)).await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/series/series-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "summary": "Updated summary from series contract"
                    })
                    .to_string(),
                ))
                .expect("series metadata patch request should build"),
        )
        .await
        .expect("series metadata patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let after_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail after metadata update request should build"),
        )
        .await
        .expect("series detail after metadata update request should complete");
    assert_eq!(after_response.status(), StatusCode::OK);
    let after_payload = response_json(after_response).await;
    let after_last_modified = after_payload
        .get("lastModified")
        .and_then(Value::as_str)
        .expect("series detail payload should expose lastModified after metadata update");
    assert_ne!(after_last_modified, before_last_modified);
    assert_eq!(
        after_payload
            .get("metadata")
            .and_then(|metadata| metadata.get("summary")),
        Some(&Value::String(
            "Updated summary from series contract".to_string()
        )),
    );
}

#[tokio::test]
async fn router_discovery_series_metadata_update_supports_extended_field_coverage() {
    let ctx = TestFixture::new("router-discovery-series-metadata-extended-fields").await;
    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/series/series-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "status": "ENDED",
                        "statusLock": true,
                        "title": "Series 1 Updated",
                        "titleLock": true,
                        "titleSort": "Series 1 Updated Sort",
                        "titleSortLock": true,
                        "language": "FR",
                        "languageLock": true,
                        "publisher": "Updated Pub",
                        "publisherLock": true,
                        "readingDirection": "RIGHT_TO_LEFT",
                        "readingDirectionLock": true,
                        "summary": "Updated summary from extended metadata test",
                        "summaryLock": true,
                        "ageRating": null,
                        "ageRatingLock": true,
                        "genres": ["Drama", "Mystery"],
                        "genresLock": true,
                        "tags": ["Pinned"],
                        "tagsLock": true,
                        "sharingLabels": ["Team", "Staff"],
                        "sharingLabelsLock": true,
                        "links": [
                            {"label": "AniList", "url": "https://anilist.co/series/1"}
                        ],
                        "linksLock": true,
                        "alternateTitles": [
                            {"label": "ja-ro", "title": "Alt A"},
                            {"label": "en", "title": "Alt B"}
                        ],
                        "alternateTitlesLock": true,
                        "totalBookCount": 7,
                        "totalBookCountLock": true
                    })
                    .to_string(),
                ))
                .expect("extended series metadata patch request should build"),
        )
        .await
        .expect("extended series metadata patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let detail_response = ctx
        .app().clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail after extended metadata patch should build"),
        )
        .await
        .expect("series detail after extended metadata patch should complete");
    assert_eq!(detail_response.status(), StatusCode::OK);
    let payload = response_json(detail_response).await;
    let metadata = payload
        .get("metadata")
        .expect("series detail payload should expose metadata");

    assert_eq!(
        metadata.get("status"),
        Some(&Value::String("ENDED".to_string()))
    );
    assert_eq!(metadata.get("statusLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("title"),
        Some(&Value::String("Series 1 Updated".to_string()))
    );
    assert_eq!(metadata.get("titleLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("titleSort"),
        Some(&Value::String("Series 1 Updated Sort".to_string()))
    );
    assert_eq!(metadata.get("titleSortLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("language"),
        Some(&Value::String("FR".to_string()))
    );
    assert_eq!(metadata.get("languageLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("publisher"),
        Some(&Value::String("Updated Pub".to_string()))
    );
    assert_eq!(metadata.get("publisherLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("readingDirection"),
        Some(&Value::String("RIGHT_TO_LEFT".to_string()))
    );
    assert_eq!(
        metadata.get("readingDirectionLock"),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        metadata.get("summary"),
        Some(&Value::String(
            "Updated summary from extended metadata test".to_string()
        ))
    );
    assert_eq!(metadata.get("summaryLock"), Some(&Value::Bool(true)));
    assert_eq!(metadata.get("ageRating"), Some(&Value::Null));
    assert_eq!(metadata.get("ageRatingLock"), Some(&Value::Bool(true)));
    assert_eq!(metadata.get("genres"), Some(&json!(["Drama", "Mystery"])));
    assert_eq!(metadata.get("genresLock"), Some(&Value::Bool(true)));
    assert_eq!(metadata.get("tags"), Some(&json!(["Pinned"])));
    assert_eq!(metadata.get("tagsLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("sharingLabels"),
        Some(&json!(["Team", "Staff"]))
    );
    assert_eq!(metadata.get("sharingLabelsLock"), Some(&Value::Bool(true)));
    assert_eq!(
        metadata.get("links"),
        Some(&json!([
            {"label": "AniList", "url": "https://anilist.co/series/1"}
        ]))
    );
    assert_eq!(metadata.get("linksLock"), Some(&Value::Bool(true)));
    let mut alternate_titles = metadata
        .get("alternateTitles")
        .and_then(Value::as_array)
        .cloned()
        .expect("series metadata should expose alternateTitles array");
    alternate_titles.sort_by_key(|value| value.to_string());
    let mut expected_alternate_titles = vec![
        json!({"label": "ja-ro", "title": "Alt A"}),
        json!({"label": "en", "title": "Alt B"}),
    ];
    expected_alternate_titles.sort_by_key(|value| value.to_string());
    assert_eq!(alternate_titles, expected_alternate_titles);
    assert_eq!(
        metadata.get("alternateTitlesLock"),
        Some(&Value::Bool(true))
    );
    assert_eq!(metadata.get("totalBookCount"), Some(&json!(7)));
    assert_eq!(metadata.get("totalBookCountLock"), Some(&Value::Bool(true)));
}

#[tokio::test]
async fn router_discovery_series_metadata_update_rejects_invalid_scalar_values() {
    let ctx = TestFixture::new("router-discovery-series-metadata-invalid-scalars").await;
    let auth_token = ctx.login_admin().await;

    for payload in [
        json!({ "status": "BROKEN_STATUS" }),
        json!({ "readingDirection": "UPSIDE_DOWN" }),
        json!({ "title": "" }),
        json!({ "titleSort": "" }),
        json!({ "totalBookCount": 0 }),
        json!({ "totalBookCount": 2147483648u64 }),
        json!({ "language": "en_US" }),
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/series/series-1/metadata")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("invalid scalar metadata patch request should build"),
            )
            .await
            .expect("invalid scalar metadata patch request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn router_discovery_series_metadata_update_rejects_invalid_structured_values() {
    let ctx = TestFixture::new("router-discovery-series-metadata-invalid-structured").await;
    let auth_token = ctx.login_admin().await;

    for payload in [
        json!({
            "links": [
                {"label": "", "url": "https://anilist.co/series/1"}
            ]
        }),
        json!({
            "links": [
                {"label": "AniList", "url": "not-a-url"}
            ]
        }),
        json!({
            "alternateTitles": [
                {"label": "", "title": "Alt A"}
            ]
        }),
        json!({
            "alternateTitles": [
                {"label": "en", "title": ""}
            ]
        }),
    ] {
        let response = ctx
            .app()
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/series/series-1/metadata")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(payload.to_string()))
                    .expect("invalid structured metadata patch request should build"),
            )
            .await
            .expect("invalid structured metadata patch request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn router_discovery_series_metadata_update_accepts_large_age_rating_within_kotlin_int_range()
{
    let ctx = TestFixture::new("router-discovery-series-metadata-large-age-rating").await;
    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/series/series-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "ageRating": 70000 }).to_string()))
                .expect("large age rating patch request should build"),
        )
        .await
        .expect("large age rating patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let detail_response = ctx
        .app().clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail after large age rating patch should build"),
        )
        .await
        .expect("series detail after large age rating patch should complete");
    assert_eq!(detail_response.status(), StatusCode::OK);
    let payload = response_json(detail_response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("ageRating")),
        Some(&json!(70000))
    );
}

#[tokio::test]
async fn router_discovery_series_metadata_update_clamps_legacy_numeric_values_to_kotlin_int_range()
{
    let ctx = TestFixture::builder(
        "router-discovery-series-metadata-clamps-legacy-numeric-values",
    )
    .with_seed(|paths| async move {
        let pool = connect_test_pool(paths.main_db.as_path(), 1)
            .await
            .expect("legacy numeric parity db should open");
        sqlx::query(
            "UPDATE SERIES_METADATA SET AGE_RATING = ?, TOTAL_BOOK_COUNT = ? WHERE SERIES_ID = ?",
        )
        .bind(3_000_000_000_i64)
        .bind(3_000_000_000_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("legacy numeric parity values should be written");
        pool.close().await;
    })
    .build()
    .await;
    let auth_token = ctx.login_admin().await;

    let patch_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/series/series-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "summary": "Clamp legacy oversized numeric metadata" }).to_string(),
                ))
                .expect("legacy numeric clamp patch request should build"),
        )
        .await
        .expect("legacy numeric clamp patch request should complete");
    assert_eq!(patch_response.status(), StatusCode::NO_CONTENT);

    let detail_response = ctx
        .app().clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/series/series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("series detail after legacy numeric clamp patch should build"),
        )
        .await
        .expect("series detail after legacy numeric clamp patch should complete");
    assert_eq!(detail_response.status(), StatusCode::OK);
    let payload = response_json(detail_response).await;
    let metadata = payload
        .get("metadata")
        .expect("series detail payload should expose metadata");
    assert_eq!(metadata.get("ageRating"), Some(&json!(2_147_483_647u32)));
    assert_eq!(
        metadata.get("totalBookCount"),
        Some(&json!(2_147_483_647u32))
    );
}
