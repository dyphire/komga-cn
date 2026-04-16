use super::*;

#[tokio::test]
async fn router_readlist_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-readlist-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("readlist thumbnail upload request should build"),
        )
        .await
        .expect("readlist thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("readListId"),
        Some(&Value::String("readlist-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(payload.get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        payload.get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(payload.get("width"), Some(&json!(1)));
    assert_eq!(payload.get("height"), Some(&json!(1)));

    let thumbnails = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnails request should build"),
        )
        .await
        .expect("readlist thumbnails request should complete");

    assert_eq!(thumbnails.status(), StatusCode::OK);
    let thumbnail_rows = response_json(thumbnails).await;
    let rows = thumbnail_rows
        .as_array()
        .expect("readlist thumbnails payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("readListId"),
        Some(&Value::String("readlist-1".to_string()))
    );
    assert_eq!(
        rows[0].get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(rows[0].get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        rows[0].get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        rows[0].get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(rows[0].get("width"), Some(&json!(1)));
    assert_eq!(rows[0].get("height"), Some(&json!(1)));

    let route_thumbnail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail route request should build"),
        )
        .await
        .expect("readlist thumbnail route request should complete");

    assert_eq!(route_thumbnail.status(), StatusCode::OK);
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let route_thumbnail_body = to_bytes(route_thumbnail.into_body(), usize::MAX)
        .await
        .expect("readlist thumbnail route body should be readable");
    assert_ne!(route_thumbnail_body.as_ref(), image_bytes.as_slice());
    assert_eq!(&route_thumbnail_body[..3], &[0xFF, 0xD8, 0xFF]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_delete_removes_uploaded_thumbnail() {
    let paths = new_router_fixture("router-readlist-thumbnail-delete-success").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("readlist thumbnail upload request should build"),
        )
        .await
        .expect("readlist thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("uploaded readlist thumbnail should expose id")
        .to_string();

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/readlists/readlist-1/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail delete request should build"),
        )
        .await
        .expect("readlist thumbnail delete request should complete");
    assert_eq!(delete.status(), StatusCode::ACCEPTED);

    let list = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail list request should build"),
        )
        .await
        .expect("readlist thumbnail list request should complete");
    assert_eq!(list.status(), StatusCode::OK);
    assert_eq!(response_json(list).await, json!([]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_and_collection_thumbnail_routes_accept_basic_auth_like_kotlin_clients() {
    let paths = new_router_fixture("router-readlist-collection-thumbnails-basic-auth-compat").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let authorization =
        basic_authorization_header_value("admin@example.org", "router-contract-admin-123");

    let (readlist_content_type, readlist_body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", true, &image_bytes);
    let readlist_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, readlist_content_type)
                .body(Body::from(readlist_body))
                .expect("basic-auth readlist thumbnail upload should build"),
        )
        .await
        .expect("basic-auth readlist thumbnail upload should complete");
    assert_eq!(readlist_upload.status(), StatusCode::OK);
    let readlist_thumbnail_id = response_json(readlist_upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("basic-auth readlist upload should return thumbnail id")
        .to_string();

    let (collection_content_type, collection_body) =
        multipart_image_upload_body("file", "collection.png", "image/png", true, &image_bytes);
    let collection_upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, collection_content_type)
                .body(Body::from(collection_body))
                .expect("basic-auth collection thumbnail upload should build"),
        )
        .await
        .expect("basic-auth collection thumbnail upload should complete");
    assert_eq!(collection_upload.status(), StatusCode::OK);
    let collection_thumbnail_id = response_json(collection_upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("basic-auth collection upload should return thumbnail id")
        .to_string();

    for route in [
        "/api/v1/readlists/readlist-1/thumbnail".to_string(),
        "/api/v1/readlists/readlist-1/thumbnails".to_string(),
        format!("/api/v1/readlists/readlist-1/thumbnails/{readlist_thumbnail_id}"),
        "/api/v1/collections/collection-1/thumbnail".to_string(),
        "/api/v1/collections/collection-1/thumbnails".to_string(),
        format!("/api/v1/collections/collection-1/thumbnails/{collection_thumbnail_id}"),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route.as_str())
                    .header(header::AUTHORIZATION, authorization.as_str())
                    .header("x-auth-token", "")
                    .body(Body::empty())
                    .expect("thumbnail basic-auth request should build"),
            )
            .await
            .expect("thumbnail basic-auth request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_select_marks_uploaded_thumbnail_selected() {
    let paths = new_router_fixture("router-readlist-thumbnail-select-success").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "readlist.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("readlist thumbnail upload request should build"),
        )
        .await
        .expect("readlist thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("uploaded readlist thumbnail should expose id")
        .to_string();

    let select = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/v1/readlists/readlist-1/thumbnails/{thumbnail_id}/selected"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail select request should build"),
        )
        .await
        .expect("readlist thumbnail select request should complete");
    assert_eq!(select.status(), StatusCode::ACCEPTED);

    let list = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail list request should build"),
        )
        .await
        .expect("readlist thumbnail list request should complete");
    assert_eq!(list.status(), StatusCode::OK);
    let rows = response_json(list).await;
    let rows = rows
        .as_array()
        .expect("readlist thumbnail list response should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("id"), Some(&Value::String(thumbnail_id)));
    assert_eq!(rows[0].get("selected"), Some(&Value::Bool(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_select_returns_accepted_when_thumbnail_is_missing_but_readlist_exists()
 {
    let paths = new_router_fixture("router-readlist-thumbnail-select-missing-thumbnail").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/readlists/readlist-1/thumbnails/missing-thumbnail/selected")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail select missing-thumbnail request should build"),
        )
        .await
        .expect("readlist thumbnail select missing-thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-collection-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "collection.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("collection thumbnail upload request should build"),
        )
        .await
        .expect("collection thumbnail upload request should complete");

    assert_eq!(upload.status(), StatusCode::OK);
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("collectionId"),
        Some(&Value::String("collection-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert_eq!(payload.get("selected"), Some(&Value::Bool(false)));
    assert_eq!(
        payload.get("mediaType"),
        Some(&Value::String("image/png".to_string()))
    );
    assert_eq!(
        payload.get("fileSize"),
        Some(&json!(image_bytes.len() as i64))
    );
    assert_eq!(payload.get("width"), Some(&json!(1)));
    assert_eq!(payload.get("height"), Some(&json!(1)));

    let thumbnail_id = payload
        .get("id")
        .and_then(Value::as_str)
        .expect("collection thumbnail upload should return thumbnail id");
    let stored = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/collections/collection-1/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail fetch request should build"),
        )
        .await
        .expect("collection thumbnail fetch request should complete");

    assert_eq!(stored.status(), StatusCode::OK);
    assert_eq!(
        stored
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/png")
    );

    let route_thumbnail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail route request should build"),
        )
        .await
        .expect("collection thumbnail route request should complete");

    assert_eq!(route_thumbnail.status(), StatusCode::OK);
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        route_thumbnail
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let route_thumbnail_body = to_bytes(route_thumbnail.into_body(), usize::MAX)
        .await
        .expect("collection thumbnail route body should be readable");
    assert_ne!(route_thumbnail_body.as_ref(), image_bytes.as_slice());
    assert_eq!(&route_thumbnail_body[..3], &[0xFF, 0xD8, 0xFF]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnail_select_returns_not_found_when_path_collection_missing() {
    let paths =
        new_router_fixture("router-collection-thumbnail-select-missing-path-collection").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "collection.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("collection thumbnail upload request should build"),
        )
        .await
        .expect("collection thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("uploaded collection thumbnail should expose id")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri(format!(
                    "/api/v1/collections/collection-missing/thumbnails/{thumbnail_id}/selected"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail select missing-path request should build"),
        )
        .await
        .expect("collection thumbnail select missing-path request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_delete_removes_persisted_thumbnails() {
    let paths = new_router_fixture("router-collection-delete-removes-thumbnails").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "collection.png", "image/png", false, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("collection thumbnail upload request should build"),
        )
        .await
        .expect("collection thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let delete = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/collections/collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection delete request should build"),
        )
        .await
        .expect("collection delete request should complete");
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for collection thumbnail verification");
    let remaining =
        sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_COLLECTION WHERE COLLECTION_ID = ?")
            .bind("collection-1")
            .fetch_one(&verify_pool)
            .await
            .expect("collection thumbnails should be queryable")
            .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(remaining, 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_readlist_thumbnail_falls_back_to_dynamic_mosaic_when_no_persisted_thumbnail_exists()
{
    let paths = new_router_fixture("router-readlist-thumbnail-mosaic-fallback").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "book.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("book thumbnail upload request should build"),
        )
        .await
        .expect("book thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/readlists/readlist-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("readlist thumbnail request should build"),
        )
        .await
        .expect("readlist thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("readlist thumbnail body should be readable");
    assert!(
        !body.is_empty(),
        "readlist mosaic thumbnail should not be empty"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnails_allow_partially_visible_collection() {
    let paths = new_router_fixture("router-collection-thumbnails-partially-visible").await;
    seed_router_contract_data(&paths).await;
    seed_collection_series_variants(&paths).await;
    seed_router_library_restricted_user(
        &paths,
        "library-1-user",
        "library1@example.org",
        "router-contract-library1-123",
        &["library-1"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "library1@example.org",
        "router-contract-library1-123",
    )
    .await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "collection.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &admin_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("partially visible collection thumbnail upload request should build"),
        )
        .await
        .expect("partially visible collection thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/thumbnails")
                .header("x-auth-token", &restricted_token)
                .body(Body::empty())
                .expect("partially visible collection thumbnails request should build"),
        )
        .await
        .expect("partially visible collection thumbnails request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload.as_array().map(Vec::len), Some(1));
    assert_eq!(
        payload[0].get("collectionId").and_then(Value::as_str),
        Some("collection-1")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_collection_thumbnail_falls_back_to_dynamic_mosaic_when_no_persisted_thumbnail_exists()
 {
    let paths = new_router_fixture("router-collection-thumbnail-mosaic-fallback").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "series.png", "image/png", true, &image_bytes);

    let upload = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/series/series-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, content_type)
                .body(Body::from(body))
                .expect("series thumbnail upload request should build"),
        )
        .await
        .expect("series thumbnail upload request should complete");
    assert_eq!(upload.status(), StatusCode::OK);

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/collections/collection-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("collection thumbnail request should build"),
        )
        .await
        .expect("collection thumbnail request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );
    assert_eq!(
        response
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("max-age=3600, private")
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("collection thumbnail body should be readable");
    assert!(
        !body.is_empty(),
        "collection mosaic thumbnail should not be empty"
    );

    cleanup_router_fixture(paths);
}
