#![allow(clippy::await_holding_lock)]

use super::*;

fn fixed_layout_extension_blob() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 100, 225, 210, 105, 2, 255, 171, 86, 202, 44, 118, 203, 172, 72, 77, 241,
        73, 172, 204, 47, 45, 81, 178, 42, 41, 42, 77, 173, 5, 0, 254, 47, 201, 165, 22, 0, 0, 0,
    ]
}

#[tokio::test]
async fn router_kobo_book_metadata_route_sets_etag_and_supports_if_none_match() {
    let ctx = TestFixture::builder("router-kobo-book-metadata-cache-headers")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let first_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata request should build"),
        )
        .await
        .expect("kobo metadata request should complete");

    assert_eq!(first_response.status(), StatusCode::OK);
    let etag = first_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("kobo metadata response should include etag");

    let second_response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::IF_NONE_MATCH, etag)
                .body(Body::empty())
                .expect("conditional kobo metadata request should build"),
        )
        .await
        .expect("conditional kobo metadata request should complete");

    assert_eq!(second_response.status(), StatusCode::NOT_MODIFIED);
    assert!(second_response.headers().contains_key(header::ETAG));
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_persisted_fields_instead_of_placeholders() {
    let ctx = TestFixture::builder("router-kobo-book-metadata-parity")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("router contract db should open for kobo metadata parity");
    sqlx::query("UPDATE BOOK_METADATA SET ISBN = ? WHERE BOOK_ID = ?")
        .bind("9781234567890")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata isbn should be updated");
    sqlx::query("UPDATE MEDIA SET EPUB_IS_KEPUB = ? WHERE BOOK_ID = ?")
        .bind(false)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata epub is kepub should be updated");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata parity request should build"),
        )
        .await
        .expect("kobo metadata parity request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("kobo metadata response should contain one item");

    assert_eq!(metadata.get("Description"), Some(&json!(" ")));
    assert_eq!(metadata.get("Language"), Some(&json!("en")));
    assert_eq!(metadata.get("CoverImageId"), Some(&json!("thumb-book-1")));
    assert_eq!(metadata.get("ISBN"), Some(&json!("9781234567890")));
    assert_eq!(
        metadata.pointer("/Publisher/Name"),
        Some(&json!("PubHouse"))
    );
    assert_eq!(metadata.pointer("/Publisher/Imprint"), Some(&json!("")));
    assert_eq!(metadata.pointer("/Series/Id"), Some(&json!("series-1")));
    assert_eq!(metadata.pointer("/Series/Name"), Some(&json!("Series 1")));
    assert_eq!(metadata.pointer("/Series/Number"), Some(&json!("1")));
    assert_eq!(metadata.pointer("/Series/NumberFloat"), Some(&json!(1.0)));
    assert_eq!(metadata.get("Contributors"), Some(&json!(["Jane Writer"])));
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Format"),
        Some(&json!("KEPUB"))
    );
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(format!(
            "http://localhost:{}/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=true",
            ctx.config().bind_address.port()
        )))
    );
    assert_eq!(
        metadata.pointer("/ContributorRoles/0/Name"),
        Some(&json!("Jane Writer"))
    );
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_epub3fl_for_fixed_layout_books() {
    let ctx = TestFixture::builder("router-kobo-book-metadata-fixed-layout")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("router contract db should open for fixed-layout metadata parity");
    sqlx::query("UPDATE MEDIA SET EPUB_IS_KEPUB = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind(false)
        .bind(fixed_layout_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata media extension should be updated");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo fixed-layout metadata request should build"),
        )
        .await
        .expect("kobo fixed-layout metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("fixed-layout metadata response should contain one item");
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Format"),
        Some(&json!("EPUB3FL"))
    );
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(format!(
            "http://localhost:{}/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=false",
            ctx.config().bind_address.port()
        )))
    );
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_kobo_port_when_host_omits_port() {
    let ctx = TestFixture::builder("router-kobo-book-metadata-kobo-port")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
            upsert_server_setting(&paths, "KOBO_PORT", "8085").await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header(header::HOST, "localhost")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo metadata koboPort request should build"),
        )
        .await
        .expect("kobo metadata koboPort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("kobo metadata response should contain one item");
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(
            "http://localhost:8085/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=true"
        ))
    );
}

#[tokio::test]
async fn router_kobo_book_metadata_returns_empty_array_when_book_is_missing_and_proxy_disabled() {
    let ctx = TestFixture::builder("router-kobo-book-metadata-missing-local")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/missing-book/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing local metadata request should build"),
        )
        .await
        .expect("kobo missing local metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!([]));
}

#[tokio::test]
async fn router_kobo_book_metadata_returns_empty_array_when_book_exists_but_metadata_row_is_missing()
 {
    let _guard = kobo_proxy_env_lock().lock().await;
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"[{"Title":"Proxy Title","DownloadUrls":[{"Format":"EPUB3","Url":"https://proxy.example/book.epub"}]}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::builder("router-kobo-book-metadata-missing-metadata-row")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
            upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
        })
        .build()
        .await;

    let pool = connect_test_pool(ctx.paths().main_db.as_path(), 1)
        .await
        .expect("router contract db should open for missing metadata row");
    sqlx::query("DELETE FROM BOOK_METADATA WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata row should be deleted");
    pool.close().await;

    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing metadata row request should build"),
        )
        .await
        .expect("kobo missing metadata row request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response_json(response).await, json!([]));

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server.join.abort();
}

#[tokio::test]
async fn router_kobo_book_metadata_proxies_missing_books_when_proxy_enabled() {
    let _guard = kobo_proxy_env_lock().lock().await;
    let previous = std::env::var("KOMGA_RUST_KOBO_PROXY_URL").ok();

    let server = spawn_single_response_server(
        200,
        "application/json",
        r#"[{"Title":"Proxy Title","DownloadUrls":[{"Format":"EPUB3","Url":"https://proxy.example/book.epub"}]}]"#,
    )
    .await;
    unsafe {
        std::env::set_var("KOMGA_RUST_KOBO_PROXY_URL", server.url.clone());
    }

    let ctx = TestFixture::builder("router-kobo-book-metadata-proxy-missing")
        .with_seed(|paths| async move {
            seed_admin_kobo_path_token(&paths).await;
            upsert_server_setting(&paths, "KOBO_PROXY", "true").await;
        })
        .build()
        .await;
    let auth_token = ctx.login_admin().await;

    let response = ctx
        .app()
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/missing-book/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo missing proxied metadata request should build"),
        )
        .await
        .expect("kobo missing proxied metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response_json(response).await,
        json!([{"Title":"Proxy Title","DownloadUrls":[{"Format":"EPUB3","Url":"https://proxy.example/book.epub"}]}])
    );

    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo missing proxied metadata server should finish");
}
