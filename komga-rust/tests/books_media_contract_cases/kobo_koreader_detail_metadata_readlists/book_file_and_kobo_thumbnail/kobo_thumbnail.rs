use super::*;

async fn seed_kobo_thumbnail_bytes(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    bytes: &[u8],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for kobo thumbnail seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(bytes)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("kobo thumbnail row should be updated");
    pool.close().await;
}

async fn seed_kobo_thumbnail_sidecar_url(
    paths: &RuntimeDbPaths,
    thumbnail_id: &str,
    media_type: &str,
    relative_path: &str,
    bytes: &[u8],
) {
    let sidecar_path = paths.config_dir.join(relative_path);
    if let Some(parent) = sidecar_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("kobo thumbnail sidecar parent directory should be created");
    }
    std::fs::write(&sidecar_path, bytes).expect("kobo thumbnail sidecar file should be written");
    let sidecar_url = reqwest::Url::from_file_path(sidecar_path.as_path())
        .expect("kobo thumbnail sidecar path should convert to file url")
        .to_string();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for kobo thumbnail sidecar seed");
    sqlx::query("UPDATE THUMBNAIL_BOOK SET MEDIA_TYPE = ?, THUMBNAIL = NULL, URL = ? WHERE ID = ?")
        .bind(media_type)
        .bind(sidecar_url)
        .bind(thumbnail_id)
        .execute(&pool)
        .await
        .expect("kobo thumbnail sidecar row should be updated");
    pool.close().await;
}

#[tokio::test]
async fn router_kobo_thumbnail_exact_id_local_response_is_jpeg() {
    let paths = new_router_fixture("router-kobo-thumbnail-local-jpeg").await;
    seed_router_contract_data(&paths).await;
    seed_admin_kobo_path_token(&paths).await;
    seed_kobo_thumbnail_bytes(&paths, "thumb-book-1", "image/png", &fixture_png_bytes()).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/thumb-book-1/thumbnail/800/800/false/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail local request should build"),
        )
        .await
        .expect("kobo thumbnail local request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo thumbnail local response body should be readable");
    assert_eq!(
        image::guess_format(body.as_ref()).expect("kobo thumbnail local body should decode"),
        image::ImageFormat::Jpeg
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_redirects_to_kobo_cdn_when_exact_thumbnail_is_missing_and_proxy_enabled()
 {
    let paths = new_router_fixture("router-kobo-thumbnail-redirects-to-cdn").await;
    seed_router_contract_data(&paths).await;
    seed_admin_kobo_path_token(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/thumbnail/800/800/90/true/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail redirect request should build"),
        )
        .await
        .expect("kobo thumbnail redirect request should complete");

    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(
        response
            .headers()
            .get(header::LOCATION)
            .and_then(|value| value.to_str().ok()),
        Some("https://cdn.kobo.com/book-images/book-1/800/800/false/image.jpg")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_returns_not_found_when_exact_thumbnail_is_missing_and_proxy_disabled()
 {
    let paths = new_router_fixture("router-kobo-thumbnail-missing-local").await;
    seed_router_contract_data(&paths).await;
    seed_admin_kobo_path_token(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/book-1/thumbnail/800/800/false/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail missing local request should build"),
        )
        .await
        .expect("kobo thumbnail missing local request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_thumbnail_exact_id_sidecar_stays_local_when_proxy_enabled() {
    let paths = new_router_fixture("router-kobo-thumbnail-sidecar-local").await;
    seed_router_contract_data(&paths).await;
    seed_admin_kobo_path_token(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
    seed_kobo_thumbnail_sidecar_url(
        &paths,
        "thumb-book-1",
        "image/png",
        "covers/thumb-book-1.png",
        &fixture_png_bytes(),
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/books/thumb-book-1/thumbnail/800/800/90/true/image.jpg")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo thumbnail sidecar request should build"),
        )
        .await
        .expect("kobo thumbnail sidecar request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert!(
        !response.headers().contains_key(header::LOCATION),
        "exact thumbnail id should stay local even when Kobo proxy is enabled"
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("kobo thumbnail sidecar response body should be readable");
    assert_eq!(
        image::guess_format(body.as_ref()).expect("kobo thumbnail sidecar body should decode"),
        image::ImageFormat::Jpeg
    );

    cleanup_router_fixture(paths);
}
