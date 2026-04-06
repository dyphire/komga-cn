use super::*;

struct SingleResponseServer {
    url: String,
    join: tokio::task::JoinHandle<()>,
}

fn kobo_proxy_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn restore_env_var(name: &str, previous: Option<String>) {
    match previous {
        Some(value) => unsafe { std::env::set_var(name, value) },
        None => unsafe { std::env::remove_var(name) },
    }
}

async fn spawn_single_response_server(
    status_code: u16,
    content_type: &str,
    body: &str,
) -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock response server should bind");
    let address = listener
        .local_addr()
        .expect("mock response server should have local addr");
    let body = body.to_string();
    let content_type = content_type.to_string();
    let join = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("mock response server should accept one connection");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await;
        let status_text = match status_code {
            200 => "OK",
            404 => "Not Found",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "OK",
        };
        let response = format!(
            "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("mock response server should write response");
    });

    SingleResponseServer {
        url: format!("http://{address}/feed.json"),
        join,
    }
}

fn fixed_layout_extension_blob() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 100, 225, 210, 105, 2, 255, 171, 86, 202, 44, 118, 203, 172, 72, 77, 241,
        73, 172, 204, 47, 45, 81, 178, 42, 41, 42, 77, 173, 5, 0, 254, 47, 201, 165, 22, 0, 0, 0,
    ]
}

async fn upsert_server_setting(paths: &RuntimeDbPaths, key: &str, value: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("server settings db should open");

    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("server setting should upsert");

    pool.close().await;
}

fn write_executable_fixture(paths: &RuntimeDbPaths, file_name: &str) -> String {
    let path = paths.config_dir.join(file_name);
    fs::write(&path, "#!/bin/sh\nexit 0\n").expect("kepubify fixture should be written");
    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&path)
            .expect("kepubify fixture metadata should load")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).expect("kepubify fixture should be executable");
    }
    path.to_string_lossy().to_string()
}

#[tokio::test]
async fn router_kobo_book_metadata_route_sets_etag_and_supports_if_none_match() {
    let paths = new_router_fixture("router-kobo-book-metadata-cache-headers").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let first_response = app
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

    let second_response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_persisted_fields_instead_of_placeholders() {
    let paths = new_router_fixture("router-kobo-book-metadata-parity").await;
    seed_router_contract_data(&paths).await;
    let kepubify_path = write_executable_fixture(&paths, "kepubify-ok.sh");
    upsert_server_setting(&paths, "KEPUBIFY_PATH", &kepubify_path).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
            runtime_config_for_paths(&paths).bind_address.port()
        )))
    );
    assert_eq!(
        metadata.pointer("/ContributorRoles/0/Name"),
        Some(&json!("Jane Writer"))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_epub3fl_for_fixed_layout_books() {
    let paths = new_router_fixture("router-kobo-book-metadata-fixed-layout").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
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

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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
            runtime_config_for_paths(&paths).bind_address.port()
        )))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_uses_epub3_when_kepub_conversion_is_not_available() {
    let paths = new_router_fixture("router-kobo-book-metadata-epub3-fallback").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KEPUBIFY_PATH", "/definitely/missing/kepubify").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/metadata")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo epub3 fallback metadata request should build"),
        )
        .await
        .expect("kobo epub3 fallback metadata request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let metadata = payload
        .as_array()
        .and_then(|items| items.first())
        .expect("epub3 fallback metadata response should contain one item");
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Format"),
        Some(&json!("EPUB3"))
    );
    assert_eq!(
        metadata.pointer("/DownloadUrls/0/Url"),
        Some(&json!(format!(
            "http://localhost:{}/kobo/any-token/v1/books/book-1/file/epub?convert_kepub=false",
            runtime_config_for_paths(&paths).bind_address.port()
        )))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_returns_empty_array_when_book_is_missing_and_proxy_disabled() {
    let paths = new_router_fixture("router-kobo-book-metadata-missing-local").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_book_metadata_returns_empty_array_when_book_exists_but_metadata_row_is_missing()
 {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
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

    let paths = new_router_fixture("router-kobo-book-metadata-missing-metadata-row").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for missing metadata row");
    sqlx::query("DELETE FROM BOOK_METADATA WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book metadata row should be deleted");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server.join.abort();
}

#[tokio::test]
async fn router_kobo_book_metadata_proxies_missing_books_when_proxy_enabled() {
    let _guard = kobo_proxy_env_lock()
        .lock()
        .expect("kobo proxy env lock should not be poisoned");
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

    let paths = new_router_fixture("router-kobo-book-metadata-proxy-missing").await;
    seed_router_contract_data(&paths).await;
    upsert_server_setting(&paths, "KOBO_PROXY", "true").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
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

    cleanup_router_fixture(paths);
    restore_env_var("KOMGA_RUST_KOBO_PROXY_URL", previous);
    server
        .join
        .await
        .expect("kobo missing proxied metadata server should finish");
}
