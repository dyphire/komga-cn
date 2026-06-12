use super::*;

#[tokio::test]
async fn router_get_page_hashes_honors_match_count_desc_sort_like_kotlin() {
    let ctx = TestFixture::new("router-page-hashes-known-match-count-desc").await;
    seed_known_page_hash_samples(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?sort=matchCount,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted known page hashes request should build"),
        )
        .await
        .expect("sorted known page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("known page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("known page hash entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["gamma-hash", "alpha-hash", "beta-hash"]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);
}

#[tokio::test]
async fn router_get_page_hashes_filters_by_action_query_like_kotlin() {
    let ctx = TestFixture::new("router-page-hashes-known-action-filter").await;
    seed_known_page_hash_samples(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?action=IGNORE,DELETE_AUTO&sort=hash,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("filtered known page hashes request should build"),
        )
        .await
        .expect("filtered known page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("filtered known page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("filtered known page hash entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["alpha-hash", "beta-hash"]);
    assert_eq!(payload["totalElements"], json!(2));
}

#[tokio::test]
async fn router_get_page_hashes_maps_known_entries_to_api_page_contract() {
    let ctx = TestFixture::new("router-page-hashes-known-api-page-contract").await;
    seed_known_page_hash_samples(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?action=DELETE_AUTO&sort=hash,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hashes api page request should build"),
        )
        .await
        .expect("known page hashes api page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!({
            "content": [{
                "hash": "beta-hash",
                "size": 220,
                "action": "DELETE_AUTO",
                "deleteCount": 2,
                "matchCount": 1,
                "created": "2024-01-02T00:00:00",
                "lastModified": "2024-01-03T00:00:00"
            }],
            "pageable": {
                "pageNumber": 0,
                "pageSize": 20,
                "sort": {
                    "empty": false,
                    "sorted": true,
                    "unsorted": false
                },
                "offset": 0,
                "paged": true,
                "unpaged": false
            },
            "last": true,
            "totalElements": 1,
            "totalPages": 1,
            "first": true,
            "size": 20,
            "number": 0,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "numberOfElements": 1,
            "empty": false
        })
    );
}

#[tokio::test]
async fn router_get_page_hashes_rejects_invalid_action_query_like_kotlin() {
    let ctx = TestFixture::new("router-page-hashes-known-invalid-action").await;
    seed_known_page_hash_samples(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes?action=IGNORE,NOT_REAL")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("known page hashes invalid-action request should build"),
        )
        .await
        .expect("known page hashes invalid-action request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_hash_desc_sort_query() {
    let ctx = TestFixture::new("router-page-hashes-unknown-hash-desc-sort").await;
    seed_unknown_page_hash_samples(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown?sort=hash,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted unknown page hashes request should build"),
        )
        .await
        .expect("sorted unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    let hashes = content
        .iter()
        .map(|entry| {
            entry["hash"]
                .as_str()
                .expect("page hash unknown entry should contain hash")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(hashes, vec!["z-hash".to_string(), "a-hash".to_string()]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);
}

#[tokio::test]
async fn router_get_page_hashes_unknown_honors_kotlin_legacy_sort_keys() {
    let ctx = TestFixture::new("router-page-hashes-unknown-legacy-sort-keys").await;
    seed_unknown_page_hash_samples(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    for sort in ["url,desc", "bookId,desc", "pageNumber,desc"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/page-hashes/unknown?sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("legacy-sorted unknown page hashes request should build"),
            )
            .await
            .expect("legacy-sorted unknown page hashes request should complete");

        assert_eq!(response.status(), StatusCode::OK, "sort={sort}");
        let payload = response_json(response).await;
        let content = payload["content"]
            .as_array()
            .expect("unknown page hashes content should be an array");
        let hashes = content
            .iter()
            .map(|entry| {
                entry["hash"]
                    .as_str()
                    .expect("page hash unknown entry should contain hash")
                    .to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            hashes,
            vec!["z-hash".to_string(), "a-hash".to_string()],
            "sort={sort}"
        );
        assert_eq!(payload["sort"]["sorted"], true, "sort={sort}");
        assert_eq!(payload["sort"]["unsorted"], false, "sort={sort}");
    }
}

#[tokio::test]
async fn router_get_page_hashes_unknown_groups_same_hash_even_when_file_sizes_differ() {
    let ctx = TestFixture::new("router-page-hashes-unknown-groups-by-hash-only").await;
    seed_unknown_page_hash_samples_with_mixed_sizes(ctx.paths()).await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/unknown")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("mixed-size unknown page hashes request should build"),
        )
        .await
        .expect("mixed-size unknown page hashes request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("unknown page hashes content should be an array");
    assert_eq!(payload["totalElements"], json!(1));
    assert_eq!(content.len(), 1);
    assert_eq!(content[0]["hash"], json!("mixed-size-hash"));
    assert_eq!(content[0]["matchCount"], json!(2));
}

#[tokio::test]
async fn router_get_page_hash_matches_honors_page_number_desc_sort_query() {
    let ctx = TestFixture::new("router-page-hash-matches-page-number-desc").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=pageNumber,desc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("sorted page hash matches request should build"),
        )
        .await
        .expect("sorted page hash matches request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    let page_numbers = content
        .iter()
        .map(|entry| {
            entry["pageNumber"]
                .as_i64()
                .expect("page hash match entry should contain page number")
        })
        .collect::<Vec<_>>();
    assert_eq!(page_numbers, vec![5, 3, 1]);
    assert_eq!(payload["sort"]["sorted"], true);
    assert_eq!(payload["sort"]["unsorted"], false);
}

#[tokio::test]
async fn router_get_page_hash_matches_rejects_match_count_and_total_size_sort_keys() {
    let ctx = TestFixture::new("router-page-hash-matches-unsupported-aggregate-sort").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    for sort in ["matchCount,desc", "totalSize,desc"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/v1/page-hashes/match-sort-hash?sort={sort}"))
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("page hash matches aggregate sort request should build"),
            )
            .await
            .expect("page hash matches aggregate sort request should complete");

        assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "sort={sort}"
        );
    }
}

#[tokio::test]
async fn router_get_page_hash_matches_converts_file_url_to_path_string() {
    let ctx = TestFixture::new("router-page-hash-matches-url-to-path").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;
    update_book_url(
        ctx.paths(),
        "book-match-1",
        "file:/library-root/books/book-match-1.cbz",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches url path request should build"),
        )
        .await
        .expect("page hash matches url path request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library-root/books/book-match-1.cbz");
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_unparseable_book_url() {
    let ctx = TestFixture::new("router-page-hash-matches-invalid-url").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;
    update_book_url(ctx.paths(), "book-match-1", "::not-a-valid-url::").await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches invalid url request should build"),
        )
        .await
        .expect("page hash matches invalid url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_get_page_hash_matches_decodes_percent_encoded_file_url_path() {
    let ctx = TestFixture::new("router-page-hash-matches-decodes-file-url-path").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;
    update_book_url(
        ctx.paths(),
        "book-match-1",
        "file:/library%20root/books/book%20match%201.cbz",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches encoded file url request should build"),
        )
        .await
        .expect("page hash matches encoded file url request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload["content"]
        .as_array()
        .expect("page hash matches content should be an array");
    assert_eq!(content[0]["url"], "/library root/books/book match 1.cbz");
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_null_file_size() {
    let ctx = TestFixture::new("router-page-hash-matches-null-file-size").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;
    update_media_page_file_size_to_null(ctx.paths(), "book-match-1", 0).await;
    assert_eq!(
        load_media_page_file_size(ctx.paths(), "book-match-1", 0).await,
        None
    );

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches null file size request should build"),
        )
        .await
        .expect("page hash matches null file size request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn router_get_page_hash_matches_returns_internal_error_for_non_file_url() {
    let ctx = TestFixture::new("router-page-hash-matches-http-url").await;
    seed_page_hash_match_samples(ctx.paths(), "match-sort-hash").await;
    update_book_url(
        ctx.paths(),
        "book-match-1",
        "https://example.com/books/book-match-1.cbz",
    )
    .await;

    let app = ctx.app().clone();
    let auth_token = ctx.login_admin().await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/page-hashes/match-sort-hash?sort=bookId,asc")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("page hash matches non-file url request should build"),
        )
        .await
        .expect("page hash matches non-file url request should complete");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}
