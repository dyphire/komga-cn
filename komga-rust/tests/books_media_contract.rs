use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::application::media_assets::BookMediaRecord;
use komga_rust::config::RuntimeMode;
use komga_rust::infrastructure::filesystem::load_epub_cover_bytes;
use komga_rust::infrastructure::metadata::generate_book_thumbnail;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sqlx::Row;
use std::fs::File;
use std::io::Write;
use tower::util::ServiceExt;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[path = "support/runtime_router_contract_support.rs"]
mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[test]
fn books_media_contract_target_is_registered() {
    assert_required_target_declared("books/media", "books_media_contract");
}

fn write_router_epub_with_cover(paths: &RuntimeDbPaths, relative_book_path: &str) {
    let epub_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = epub_path.parent() {
        std::fs::create_dir_all(parent).expect("epub cover parent directory should be created");
    }

    let file = File::create(&epub_path).expect("epub cover fixture file should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("mimetype", options)
        .expect("epub cover mimetype entry should be created");
    zip.write_all(b"application/epub+zip")
        .expect("epub cover mimetype payload should be written");

    zip.start_file("META-INF/container.xml", options)
        .expect("epub cover container entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .expect("epub cover container payload should be written");

    zip.start_file("OEBPS/content.opf", options)
        .expect("epub cover package entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="bookid">book-1</dc:identifier><dc:title>Fixture Book</dc:title><dc:language>en</dc:language></metadata><manifest><item id="cover-image" href="images/cover.png" media-type="image/png" properties="cover-image"/><item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="main"/></spine></package>"#,
    )
    .expect("epub cover package payload should be written");

    zip.start_file("OEBPS/chapter.xhtml", options)
        .expect("epub cover chapter entry should be created");
    zip.write_all(
        br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>Hello</p></body></html>"#,
    )
    .expect("epub cover chapter payload should be written");

    zip.start_file("OEBPS/images/cover.png", options)
        .expect("epub cover image entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("epub cover image payload should be written");

    zip.finish()
        .expect("epub cover fixture should finish successfully");
}

fn fixture_epub_positions_extension_blob() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 189, 49, 208, 105, 2, 255, 171, 86, 42, 200, 47, 206, 44, 201, 204, 207, 43,
        86, 178, 138, 174, 86, 202, 40, 74, 77, 83, 178, 82, 210, 79, 202, 207, 207, 214, 53, 212,
        171, 200, 40, 201, 205, 81, 206, 206, 79, 202, 215, 51, 212, 51, 84, 210, 81, 42, 169, 44,
        72, 5, 202, 39, 22, 20, 228, 100, 38, 39, 130, 244, 233, 131, 213, 104, 87, 228, 230, 0,
        165, 115, 242, 33, 130, 64, 195, 170, 225, 38, 43, 89, 25, 234, 40, 21, 20, 229, 167, 23,
        165, 22, 23, 131, 249, 6, 122, 6, 64, 163, 242, 75, 18, 115, 2, 80, 133, 13, 107, 107, 117,
        240, 57, 194, 136, 2, 71, 24, 97, 56, 194, 20, 187, 35, 140, 106, 107, 99, 107, 1, 206, 33,
        248, 112, 25, 1, 0, 0,
    ]
}

fn fixture_epub_positions_extension_blob_total_progression_021() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 62, 131, 208, 105, 2, 255, 171, 86, 42, 200, 47, 206, 44, 201, 204, 207, 43,
        86, 178, 138, 174, 86, 202, 40, 74, 77, 83, 178, 82, 210, 79, 202, 207, 207, 214, 53, 212,
        171, 200, 40, 201, 205, 81, 78, 43, 74, 76, 87, 210, 81, 42, 169, 44, 72, 5, 202, 37, 22,
        20, 228, 100, 38, 39, 130, 244, 232, 131, 229, 181, 43, 114, 115, 128, 210, 57, 249, 16,
        65, 160, 65, 213, 74, 5, 69, 249, 233, 69, 169, 197, 197, 64, 190, 146, 149, 129, 158, 41,
        80, 119, 126, 73, 98, 78, 0, 170, 176, 145, 97, 109, 109, 108, 45, 0, 103, 188, 212, 29,
        132, 0, 0, 0,
    ]
}

fn fixture_epub_positions_extension_blob_total_progression_0995() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 62, 131, 208, 105, 2, 255, 171, 86, 42, 200, 47, 206, 44, 201, 204, 207, 43,
        86, 178, 138, 174, 86, 202, 40, 74, 77, 83, 178, 82, 210, 79, 202, 207, 207, 214, 53, 212,
        171, 200, 40, 201, 205, 81, 78, 43, 74, 76, 87, 210, 81, 42, 169, 44, 72, 5, 202, 37, 22,
        20, 228, 100, 38, 39, 130, 244, 232, 131, 229, 181, 43, 114, 115, 128, 210, 57, 249, 16,
        65, 160, 65, 213, 74, 5, 69, 249, 233, 69, 169, 197, 197, 64, 190, 146, 149, 129, 158, 41,
        80, 119, 126, 73, 98, 78, 0, 170, 176, 165, 165, 105, 109, 109, 108, 45, 0, 22, 101, 99, 4,
        133, 0, 0, 0,
    ]
}

fn fixture_epub_positions_extension_blob_fixed_layout_single_position() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 75, 133, 208, 105, 2, 255, 85, 141, 189, 14, 194, 32, 20, 133, 223, 229, 58,
        74, 91, 53, 113, 225, 1, 156, 28, 76, 28, 141, 3, 85, 104, 73, 105, 239, 13, 220, 38, 52,
        132, 119, 23, 116, 114, 60, 231, 59, 63, 9, 8, 131, 101, 139, 75, 0, 249, 72, 48, 122, 109,
        64, 66, 215, 35, 78, 205, 177, 141, 35, 207, 110, 103, 188, 26, 64, 0, 111, 164, 11, 83,
        68, 206, 190, 84, 237, 116, 95, 190, 143, 179, 43, 120, 194, 30, 239, 164, 150, 18, 49, 54,
        234, 119, 19, 170, 16, 224, 240, 23, 46, 7, 9, 200, 227, 224, 117, 8, 69, 131, 60, 180,
        231, 178, 138, 172, 220, 237, 223, 62, 229, 252, 20, 96, 195, 165, 238, 92, 213, 134, 43,
        131, 100, 191, 234, 252, 1, 224, 110, 213, 153, 176, 0, 0, 0,
    ]
}

async fn seed_router_cbz_book(paths: &RuntimeDbPaths, book_id: &str, file_name: &str, title: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for cbz book seed");

    let relative_path = format!("books/{file_name}");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(&relative_path)
    .bind("series-1")
    .bind(4_096_i64)
    .bind(3_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("cbz book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/vnd.comicbook+zip")
    .bind("READY")
    .bind(book_id)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("cbz media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("3")
    .bind(3.0_f64)
    .bind(title)
    .bind("2024-03-01")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("cbz book metadata row should be inserted");

    pool.close().await;

    let archive_path = paths.config_dir.join(relative_path);
    if let Some(parent) = archive_path.parent() {
        std::fs::create_dir_all(parent).expect("cbz parent directory should be created");
    }
    let file = File::create(&archive_path).expect("cbz fixture file should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);
    zip.start_file("page-1.png", options)
        .expect("cbz page entry should be created");
    zip.write_all(&fixture_png_bytes())
        .expect("cbz page payload should be written");
    zip.finish()
        .expect("cbz fixture should finish successfully");
}

#[tokio::test]
async fn router_opds_v2_divina_manifest_uses_page_media_type_in_reading_order() {
    let paths = new_router_fixture("router-opds-v2-divina-manifest-page-media-type").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;

    let mut config = runtime_config_for_paths(&paths);
    config.mode = RuntimeMode::Isolated;
    let app = build_router_with_config(&config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-3/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds v2 divina manifest request should build"),
        )
        .await
        .expect("opds v2 divina manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/png")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_divina_uses_page_media_type_in_reading_order() {
    let paths = new_router_fixture("router-book-manifest-divina-page-media-type").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-3", "book-3.cbz", "Book 3").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-3/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("v1 divina manifest request should build"),
        )
        .await
        .expect("v1 divina manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-3/pages/1?contentNegotiation=false")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/png")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_pdf_uses_raw_pdf_pages_in_reading_order() {
    let paths = new_router_fixture("router-book-manifest-pdf-reading-order").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/manifest/pdf")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf manifest request should build"),
        )
        .await
        .expect("pdf manifest request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let reading_order = payload
        .get("readingOrder")
        .and_then(Value::as_array)
        .expect("pdf manifest should expose readingOrder array");
    assert_eq!(reading_order.len(), 1);
    assert_eq!(
        reading_order[0].get("href").and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-pdf-1/pages/1/raw")
    );
    assert_eq!(
        reading_order[0].get("type").and_then(Value::as_str),
        Some("application/pdf")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_pdf_returns_bad_request_with_message_for_non_pdf_media() {
    let paths = new_router_fixture("router-book-manifest-pdf-profile-mismatch").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/manifest/pdf")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf manifest profile mismatch request should build"),
        )
        .await
        .expect("pdf manifest profile mismatch request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Book media type 'application/epub+zip' not compatible with requested profile"
                .to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_manifest_divina_accepts_pdf_books() {
    let paths = new_router_fixture("router-book-manifest-divina-pdf-book").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/manifest/divina")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("divina manifest pdf request should build"),
        )
        .await
        .expect("divina manifest pdf request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/divina+json")
    );
    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .map(|v| v.len()),
        Some(1)
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("href"))
            .and_then(Value::as_str),
        Some("http://localhost/api/v1/books/book-pdf-1/pages/1?contentNegotiation=false")
    );
    assert_eq!(
        payload
            .get("readingOrder")
            .and_then(Value::as_array)
            .and_then(|entries| entries.first())
            .and_then(|entry| entry.get("type"))
            .and_then(Value::as_str),
        Some("image/jpeg")
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_returns_conflict_for_older_progression() {
    let paths = new_router_fixture("router-book-progression-put-conflict").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for progression conflict seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression conflict test");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(5_i64)
    .bind(false)
    .bind("2024-01-03 00:00:00")
    .bind("reader-1")
    .bind("KOReader")
    .bind(serde_json::to_vec(&json!({
        "href": "/book-1.xhtml#kobo.5.1",
        "type": "application/xhtml+xml",
        "locations": {
            "progression": 0.5,
            "position": 5,
            "totalProgression": 0.5
        }
    }))
    .expect("progression conflict locator should serialize"))
    .execute(&pool)
    .await
    .expect("existing read progress row for progression conflict should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-02T00:00:00Z",
                        "device": {
                            "id": "reader-2",
                            "name": "Another device"
                        },
                        "locator": {
                            "href": "/book-1.xhtml#kobo.4.1",
                            "type": "application/xhtml+xml",
                            "locations": {
                                "progression": 0.4,
                                "position": 4,
                                "totalProgression": 0.4
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("older progression put request should build"),
        )
        .await
        .expect("older progression put request should complete");

    assert_eq!(response.status(), StatusCode::CONFLICT);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Progression is older than existing".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_allows_same_modified_retry() {
    let paths = new_router_fixture("router-book-progression-put-same-modified-retry").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for same-modified retry seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for same-modified retry test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": {
            "id": "reader-9",
            "name": "Kobo Libra"
        },
        "locator": {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            },
            "koboSpan": "kobo-span-2"
        }
    });

    for attempt in 0..2 {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/api/v1/books/book-1/progression")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(progression.to_string()))
                    .expect("same-modified retry request should build"),
            )
            .await
            .expect("same-modified retry request should complete");

        assert_eq!(
            response.status(),
            StatusCode::NO_CONTENT,
            "retry attempt {attempt} should stay idempotent"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_persists_modified_device_and_locator() {
    let paths = new_router_fixture("router-book-progression-put-persists-full-payload").await;
    seed_router_contract_data(&paths).await;

    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for progression full-payload seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression full-payload test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": {
            "id": "reader-9",
            "name": "Kobo Libra"
        },
        "locator": {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            },
            "koboSpan": "kobo-span-2"
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("book progression full-payload put request should build"),
        )
        .await
        .expect("book progression full-payload put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book progression full-payload get request should build"),
        )
        .await
        .expect("book progression full-payload get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));
    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_roundtrips_on_opds_v2_route() {
    let paths = new_router_fixture("router-book-progression-opds-v2-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for opds progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for opds progression test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": {
            "id": "reader-9",
            "name": "Kobo Libra"
        },
        "locator": {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            },
            "koboSpan": "kobo-span-2"
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/opds/v2/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("opds progression put request should build"),
        )
        .await
        .expect("opds progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/opds/v2/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("opds progression get request should build"),
        )
        .await
        .expect("opds progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_without_progression() {
    let paths = new_router_fixture("router-book-progression-put-epub-missing-progression").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "chapter.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "position": 15 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression without locator progression request should build"),
        )
        .await
        .expect("epub progression without locator progression request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "location.progression is required".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_when_extension_is_missing() {
    let paths = new_router_fixture("router-book-progression-put-epub-missing-extension").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "chapter.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.3 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression without extension request should build"),
        )
        .await
        .expect("epub progression without extension request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Epub extension not found".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_epub_locator_with_non_existing_href() {
    let paths = new_router_fixture("router-book-progression-put-epub-bad-href").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for progression bad-href seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression bad-href test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-1", "name": "KOReader" },
                        "locator": {
                            "href": "ch5.xhtml#frag",
                            "type": "application/xhtml+xml",
                            "locations": { "progression": 0.3 }
                        }
                    })
                    .to_string(),
                ))
                .expect("epub progression bad href request should build"),
        )
        .await
        .expect("epub progression bad href request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Resource does not exist in book: ch5.xhtml".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_accepts_pdf_position_payload() {
    let paths = new_router_fixture("router-book-progression-put-pdf-position").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "",
            "type": "",
            "locations": { "position": 1 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("pdf progression put request should build"),
        )
        .await
        .expect("pdf progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf progression get request should build"),
        )
        .await
        .expect("pdf progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("modified"), progression.get("modified"));
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(payload.get("locator"), progression.get("locator"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_pdf_position_beyond_page_count() {
    let paths = new_router_fixture("router-book-progression-put-pdf-out-of-range").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-pdf-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-9", "name": "Kobo Libra" },
                        "locator": {
                            "href": "",
                            "type": "",
                            "locations": { "position": 2 }
                        }
                    })
                    .to_string(),
                ))
                .expect("pdf progression out-of-range request should build"),
        )
        .await
        .expect("pdf progression out-of-range request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Page argument (2) must be within 1 and book page count (1)".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_normalizes_epub_locator_from_matching_position() {
    let paths = new_router_fixture("router-book-progression-put-epub-normalizes-locator").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub progression normalization seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for progression normalization test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#custom-fragment",
            "type": "",
            "locations": {
                "progression": 0.5,
                "totalProgression": 0.9
            }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("epub progression normalization put request should build"),
        )
        .await
        .expect("epub progression normalization put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub progression normalization get request should build"),
        )
        .await
        .expect("epub progression normalization get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(payload.get("device"), progression.get("device"));
    assert_eq!(
        payload.get("locator"),
        Some(&json!({
            "href": "/book-1.xhtml#custom-fragment",
            "type": "application/xhtml+xml",
            "locations": {
                "progression": 0.5,
                "totalProgression": 0.2
            }
        }))
    );

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for normalized progression verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("normalized progression row should be queryable");
    verify_pool.close().await;
    assert_eq!(progression_row.get::<i64, _>("PAGE"), 2);
    assert!(!progression_row.get::<bool, _>("COMPLETED"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_rejects_invalid_epub_progression_between_positions() {
    let paths = new_router_fixture("router-book-progression-put-epub-invalid-progression").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for invalid epub progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for invalid progression test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "modified": "2024-01-04T05:06:07Z",
                        "device": { "id": "reader-9", "name": "Kobo Libra" },
                        "locator": {
                            "href": "/book-1.xhtml#custom-fragment",
                            "type": "application/xhtml+xml",
                            "locations": {
                                "progression": 0.9
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("invalid epub progression request should build"),
        )
        .await
        .expect("invalid epub progression request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Invalid progression".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_accepts_fixed_layout_epub_single_position() {
    let paths =
        new_router_fixture("router-book-progression-put-epub-fixed-layout-single-position").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for fixed-layout progression seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_fixed_layout_single_position())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("fixed-layout epub extension should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#other-fragment",
            "type": "",
            "locations": { "progression": 0.9 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("fixed-layout progression put request should build"),
        )
        .await
        .expect("fixed-layout progression put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("fixed-layout progression get request should build"),
        )
        .await
        .expect("fixed-layout progression get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);
    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("locator"),
        Some(&json!({
            "href": "/book-1.xhtml#other-fragment",
            "type": "application/xhtml+xml",
            "locations": {
                "progression": 0.9,
                "totalProgression": 0.2
            },
            "koboSpan": "fixed-span"
        }))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_uses_total_progression_to_round_epub_page() {
    let paths =
        new_router_fixture("router-book-progression-put-epub-rounds-total-progression").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub page-rounding seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_total_progression_021())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for page-rounding test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#frag",
            "type": "",
            "locations": { "progression": 0.5 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("epub page-rounding put request should build"),
        )
        .await
        .expect("epub page-rounding put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for page-rounding verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("page-rounding progression row should be queryable");
    verify_pool.close().await;
    assert_eq!(progression_row.get::<i64, _>("PAGE"), 2);
    assert!(!progression_row.get::<bool, _>("COMPLETED"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_put_marks_completed_when_total_progression_is_above_threshold() {
    let paths = new_router_fixture("router-book-progression-put-epub-completed-threshold").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub completion-threshold seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(fixture_epub_positions_extension_blob_total_progression_0995())
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for completion-threshold test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let progression = json!({
        "modified": "2024-01-04T05:06:07Z",
        "device": { "id": "reader-9", "name": "Kobo Libra" },
        "locator": {
            "href": "/book-1.xhtml#frag",
            "type": "",
            "locations": { "progression": 0.5 }
        }
    });

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(progression.to_string()))
                .expect("epub completion-threshold put request should build"),
        )
        .await
        .expect("epub completion-threshold put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for completion-threshold verification");
    let progression_row = sqlx::query(
        "SELECT PAGE, COMPLETED FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("completion-threshold progression row should be queryable");
    verify_pool.close().await;
    assert_eq!(progression_row.get::<i64, _>("PAGE"), 10);
    assert!(progression_row.get::<bool, _>("COMPLETED"));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_authors_v1_filters_by_search_query() {
    let paths = new_router_fixture("router-authors-v1-search").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/authors?search=jane")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors v1 search request should build"),
        )
        .await
        .expect("authors v1 search request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let authors = payload
        .as_array()
        .expect("authors v1 search payload should be an array");
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0].get("name"), Some(&json!("Jane Writer")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_authors_v1_filters_by_collection_id() {
    let paths = new_router_fixture("router-authors-v1-collection").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/authors?collection_id=collection-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors v1 collection request should build"),
        )
        .await
        .expect("authors v1 collection request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let authors = payload
        .as_array()
        .expect("authors v1 collection payload should be an array");
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0].get("name"), Some(&json!("Jane Writer")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_authors_v1_filters_by_series_id() {
    let paths = new_router_fixture("router-authors-v1-series").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/authors?series_id=series-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("authors v1 series request should build"),
        )
        .await
        .expect("authors v1 series request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let authors = payload
        .as_array()
        .expect("authors v1 series payload should be an array");
    assert_eq!(authors.len(), 1);
    assert_eq!(authors[0].get("name"), Some(&json!("Jane Writer")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_begins_with_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-operator").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "beginsWith",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list request should build"),
        )
        .await
        .expect("strict books/list beginsWith request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status beginsWith payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_download_routes_do_not_get_shallow_etag_headers() {
    let paths = new_router_fixture("router-download-routes-no-shallow-etag").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for download exclusion test");
    std::fs::write(books_dir.join("book-1.epub"), b"download-exclusion-body")
        .expect("book fixture file should be written for download exclusion test");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let libraries_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/libraries")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("libraries request should build for exclusion control"),
        )
        .await
        .expect("libraries request should complete for exclusion control");
    let cache_etag = libraries_response
        .headers()
        .get(header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("non-download route should expose etag for exclusion control");

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
        "/kobo/any-token/v1/books/book-1/file/epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_NONE_MATCH, &cache_etag)
                    .body(Body::empty())
                    .expect("download exclusion request should build"),
            )
            .await
            .expect("download exclusion request should complete");

        assert_eq!(
            response.status(),
            StatusCode::OK,
            "download route should not turn into 304: {route}",
        );
        assert!(
            !response.headers().contains_key(header::ETAG),
            "download route should not receive shallow etag: {route}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-media-status").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "is",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status request should build"),
        )
        .await
        .expect("strict books/list media-status request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books/list media-status payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_applies_default_sort_for_unknown_sort_mode_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-sort-modes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for sort in [
        "metadata.title,asc",
        "series,metadata.numberSort,asc",
        "createdDate,desc",
        "lastModifiedDate,desc",
        "metadata.releaseDate,desc",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/books/list?page=0&size=20&sort={sort}"))
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
                    .expect("strict books/list supported sort request should build"),
            )
            .await
            .expect("strict books/list supported sort request should complete");
        assert_eq!(response.status(), StatusCode::OK);
    }

    let unsupported_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20&sort=unsupported.sort,asc")
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
                .expect("strict books/list unsupported sort request should build"),
        )
        .await
        .expect("strict books/list unsupported sort request should complete");
    assert_eq!(unsupported_response.status(), StatusCode::OK);
    let unsupported_payload = response_json(unsupported_response).await;
    let unsupported_content = unsupported_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books unsupported sort payload should expose content array");
    assert_eq!(unsupported_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_media_status_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-media-status-is-not").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "READY"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media-status isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "MediaStatus",
                            "operator": "isNot",
                            "value": "UNKNOWN"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list media-status isNot kept request should build"),
        )
        .await
        .expect("strict books/list media-status isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict media-status isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_genre_condition_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-filter-combo").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Genre",
                            "operator": "is",
                            "value": "SciFi"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unsupported condition request should build"),
        )
        .await
        .expect("strict books/list genre request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict genre payload should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_condition_type_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-unknown-condition").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "UnknownBookCondition",
                            "operator": "is",
                            "value": "whatever"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unknown-condition request should build"),
        )
        .await
        .expect("strict books/list unknown-condition request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_unknown_operator_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-unknown-operator").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list unknown-operator request should build"),
        )
        .await
        .expect("strict books/list unknown-operator request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_series_metadata_conditions_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-series-metadata").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for condition in [
        json!({ "type": "Language", "operator": "is", "value": "EN" }),
        json!({ "type": "Publisher", "operator": "is", "value": "PubHouse" }),
        json!({ "type": "AgeRating", "operator": "is", "value": 16 }),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(json!({ "condition": condition }).to_string()))
                    .expect("strict books/list series metadata request should build"),
            )
            .await
            .expect("strict books/list series metadata request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict series metadata payload should expose content array");
        assert_eq!(content.len(), 1);
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String("book-1".to_string()))
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_series_id_with_query_is_not_silent_empty_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-seriesid-query").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesId",
                            "operator": "is",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId request should build"),
        )
        .await
        .expect("strict books/list seriesId request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId request should expose content array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "SeriesId",
                            "operator": "isNot",
                            "value": "series-1"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list seriesId isNot request should build"),
        )
        .await
        .expect("strict books/list seriesId isNot request should complete");

    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict seriesId isNot request should expose content array");
    assert_eq!(excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_anyof_and_allof_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-anyof-allof").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let all_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf match request should build"),
        )
        .await
        .expect("strict books/list allOf match request should complete");
    assert_eq!(all_of_match_response.status(), StatusCode::OK);
    let all_of_match_payload = response_json(all_of_match_response).await;
    let all_of_match_content = all_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf match payload should expose content array");
    assert_eq!(all_of_match_content.len(), 1);

    let all_of_miss_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AllOfBook",
                            "conditions": [
                                {"type": "LibraryId", "operator": "is", "value": "library-1"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list allOf miss request should build"),
        )
        .await
        .expect("strict books/list allOf miss request should complete");
    assert_eq!(all_of_miss_response.status(), StatusCode::OK);
    let all_of_miss_payload = response_json(all_of_miss_response).await;
    let all_of_miss_content = all_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books allOf miss payload should expose content array");
    assert_eq!(all_of_miss_content.len(), 0);

    let any_of_match_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "READY"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf match request should build"),
        )
        .await
        .expect("strict books/list anyOf match request should complete");
    assert_eq!(any_of_match_response.status(), StatusCode::OK);
    let any_of_match_payload = response_json(any_of_match_response).await;
    let any_of_match_content = any_of_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf match payload should expose content array");
    assert_eq!(any_of_match_content.len(), 1);

    let any_of_miss_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "AnyOfBook",
                            "conditions": [
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"},
                                {"type": "MediaStatus", "operator": "is", "value": "UNKNOWN"}
                            ]
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list anyOf miss request should build"),
        )
        .await
        .expect("strict books/list anyOf miss request should complete");
    assert_eq!(any_of_miss_response.status(), StatusCode::OK);
    let any_of_miss_payload = response_json(any_of_miss_response).await;
    let any_of_miss_content = any_of_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books anyOf miss payload should expose content array");
    assert_eq!(any_of_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_read_status_is_and_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-read-status").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let unread_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list unread request should build"),
        )
        .await
        .expect("strict books/list unread request should complete");
    assert_eq!(unread_response.status(), StatusCode::OK);
    let unread_payload = response_json(unread_response).await;
    let unread_content = unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict unread payload should expose content array");
    assert_eq!(unread_content.len(), 0);

    let read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "is",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read request should build"),
        )
        .await
        .expect("strict books/list read request should complete");
    assert_eq!(read_response.status(), StatusCode::OK);
    let read_payload = response_json(read_response).await;
    let read_content = read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read payload should expose content array");
    assert_eq!(read_content.len(), 1);
    assert_eq!(
        read_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let excluded_read_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "READ"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot excluded request should build"),
        )
        .await
        .expect("strict books/list read isNot excluded request should complete");
    assert_eq!(excluded_read_response.status(), StatusCode::OK);
    let excluded_read_payload = response_json(excluded_read_response).await;
    let excluded_read_content = excluded_read_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot excluded payload should expose content array");
    assert_eq!(excluded_read_content.len(), 0);

    let kept_not_unread_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReadStatus",
                            "operator": "isNot",
                            "value": "UNREAD"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list read isNot kept request should build"),
        )
        .await
        .expect("strict books/list read isNot kept request should complete");
    assert_eq!(kept_not_unread_response.status(), StatusCode::OK);
    let kept_not_unread_payload = response_json(kept_not_unread_response).await;
    let kept_not_unread_content = kept_not_unread_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict read isNot kept payload should expose content array");
    assert_eq!(kept_not_unread_content.len(), 1);
    assert_eq!(
        kept_not_unread_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_library_id_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-library-id").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list library-id match request should build"),
        )
        .await
        .expect("strict books/list library-id match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "LibraryId",
                            "operator": "is",
                            "value": "library-missing"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list library-id miss request should build"),
        )
        .await
        .expect("strict books/list library-id miss request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books library-id miss payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_deleted_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-deleted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let not_deleted_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list deleted isFalse request should build"),
        )
        .await
        .expect("strict books/list deleted isFalse request should complete");
    assert_eq!(not_deleted_response.status(), StatusCode::OK);
    let not_deleted_payload = response_json(not_deleted_response).await;
    let not_deleted_content = not_deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isFalse payload should expose content array");
    assert_eq!(not_deleted_content.len(), 1);

    let deleted_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list deleted isTrue request should build"),
        )
        .await
        .expect("strict books/list deleted isTrue request should complete");
    assert_eq!(deleted_response.status(), StatusCode::OK);
    let deleted_payload = response_json(deleted_response).await;
    let deleted_content = deleted_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books deleted isTrue payload should expose content array");
    assert_eq!(deleted_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_oneshot_filter_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-oneshot").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let oneshot_true_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list oneshot=true request should build"),
        )
        .await
        .expect("strict books/list oneshot=true request should complete");
    assert_eq!(oneshot_true_response.status(), StatusCode::OK);
    let oneshot_true_payload = response_json(oneshot_true_response).await;
    let oneshot_true_content = oneshot_true_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=true payload should expose content array");
    assert_eq!(oneshot_true_content.len(), 0);

    let oneshot_false_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
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
                .expect("strict books/list oneshot=false request should build"),
        )
        .await
        .expect("strict books/list oneshot=false request should complete");
    assert_eq!(oneshot_false_response.status(), StatusCode::OK);
    let oneshot_false_payload = response_json(oneshot_false_response).await;
    let oneshot_false_content = oneshot_false_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict oneshot=false payload should expose content array");
    assert_eq!(oneshot_false_content.len(), 1);
    assert_eq!(
        oneshot_false_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_is_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "is",
                            "value": "2024-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date match request should build"),
        )
        .await
        .expect("strict books/list release-date match request should complete");
    assert_eq!(matched_response.status(), StatusCode::OK);
    let matched_payload = response_json(matched_response).await;
    let matched_content = matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date match payload should expose content array");
    assert_eq!(matched_content.len(), 1);

    let missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "is",
                            "value": "2025-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date missing request should build"),
        )
        .await
        .expect("strict books/list release-date missing request should complete");
    assert_eq!(missing_response.status(), StatusCode::OK);
    let missing_payload = response_json(missing_response).await;
    let missing_content = missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date missing payload should expose content array");
    assert_eq!(missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_is_not_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date-is-not").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let excluded_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNot",
                            "value": "2024-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isNot excluded request should build"),
        )
        .await
        .expect("strict books/list release-date isNot excluded request should complete");
    assert_eq!(excluded_response.status(), StatusCode::OK);
    let excluded_payload = response_json(excluded_response).await;
    let excluded_content = excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date isNot excluded payload should expose content array");
    assert_eq!(excluded_content.len(), 0);

    let kept_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNot",
                            "value": "2025-01-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isNot kept request should build"),
        )
        .await
        .expect("strict books/list release-date isNot kept request should complete");
    assert_eq!(kept_response.status(), StatusCode::OK);
    let kept_payload = response_json(kept_response).await;
    let kept_content = kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict release-date isNot kept payload should expose content array");
    assert_eq!(kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_null_operators_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date-null").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let is_null_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNull"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isNull request should build"),
        )
        .await
        .expect("strict books/list release-date isNull request should complete");
    assert_eq!(is_null_response.status(), StatusCode::OK);
    let is_null_payload = response_json(is_null_response).await;
    let is_null_content = is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isNull payload should expose content array");
    assert_eq!(is_null_content.len(), 0);

    let is_not_null_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNotNull"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isNotNull request should build"),
        )
        .await
        .expect("strict books/list release-date isNotNull request should complete");
    assert_eq!(is_not_null_response.status(), StatusCode::OK);
    let is_not_null_payload = response_json(is_not_null_response).await;
    let is_not_null_content = is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isNotNull payload should expose content array");
    assert_eq!(is_not_null_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_greater_than_and_less_than_in_runtime_owned_mode()
 {
    let paths = new_router_fixture("router-discovery-books-list-strict-release-date-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let gt_matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "greaterThan",
                            "value": "2024-01-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date greaterThan match request should build"),
        )
        .await
        .expect("strict books/list release-date greaterThan match request should complete");
    assert_eq!(gt_matched_response.status(), StatusCode::OK);
    let gt_matched_payload = response_json(gt_matched_response).await;
    let gt_matched_content = gt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date greaterThan match payload should expose content array");
    assert_eq!(gt_matched_content.len(), 1);

    let gt_missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "greaterThan",
                            "value": "2024-12-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date greaterThan missing request should build"),
        )
        .await
        .expect("strict books/list release-date greaterThan missing request should complete");
    assert_eq!(gt_missing_response.status(), StatusCode::OK);
    let gt_missing_payload = response_json(gt_missing_response).await;
    let gt_missing_content = gt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date greaterThan missing payload should expose content array",
        );
    assert_eq!(gt_missing_content.len(), 0);

    let lt_matched_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "lessThan",
                            "value": "2024-12-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date lessThan match request should build"),
        )
        .await
        .expect("strict books/list release-date lessThan match request should complete");
    assert_eq!(lt_matched_response.status(), StatusCode::OK);
    let lt_matched_payload = response_json(lt_matched_response).await;
    let lt_matched_content = lt_matched_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date lessThan match payload should expose content array");
    assert_eq!(lt_matched_content.len(), 1);

    let lt_missing_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "lessThan",
                            "value": "2024-01-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date lessThan missing request should build"),
        )
        .await
        .expect("strict books/list release-date lessThan missing request should complete");
    assert_eq!(lt_missing_response.status(), StatusCode::OK);
    let lt_missing_payload = response_json(lt_missing_response).await;
    let lt_missing_content = lt_missing_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date lessThan missing payload should expose content array");
    assert_eq!(lt_missing_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_date_style_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-release-date-date-style").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "after",
                            "dateTime": "2024-01-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date after match request should build"),
        )
        .await
        .expect("strict books/list release-date after match request should complete");
    assert_eq!(after_match.status(), StatusCode::OK);
    let after_match_payload = response_json(after_match).await;
    let after_match_content = after_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date after match payload should expose content array");
    assert_eq!(after_match_content.len(), 1);

    let after_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "after",
                            "dateTime": "2024-02-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date after miss request should build"),
        )
        .await
        .expect("strict books/list release-date after miss request should complete");
    assert_eq!(after_miss.status(), StatusCode::OK);
    let after_miss_payload = response_json(after_miss).await;
    let after_miss_content = after_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date after miss payload should expose content array");
    assert_eq!(after_miss_content.len(), 0);

    let before_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "before",
                            "dateTime": "2024-02-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date before match request should build"),
        )
        .await
        .expect("strict books/list release-date before match request should complete");
    assert_eq!(before_match.status(), StatusCode::OK);
    let before_match_payload = response_json(before_match).await;
    let before_match_content = before_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date before match payload should expose content array");
    assert_eq!(before_match_content.len(), 1);

    let before_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "before",
                            "dateTime": "2024-01-01T00:00:00Z"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date before miss request should build"),
        )
        .await
        .expect("strict books/list release-date before miss request should complete");
    assert_eq!(before_miss.status(), StatusCode::OK);
    let before_miss_payload = response_json(before_miss).await;
    let before_miss_content = before_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date before miss payload should expose content array");
    assert_eq!(before_miss_content.len(), 0);

    let is_in_the_last_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isInTheLast",
                            "duration": "P10000D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isInTheLast match request should build"),
        )
        .await
        .expect("strict books/list release-date isInTheLast match request should complete");
    assert_eq!(is_in_the_last_match.status(), StatusCode::OK);
    let is_in_the_last_match_payload = response_json(is_in_the_last_match).await;
    let is_in_the_last_match_content = is_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isInTheLast match payload should expose content array");
    assert_eq!(is_in_the_last_match_content.len(), 1);

    let is_in_the_last_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isInTheLast",
                            "duration": "P1D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isInTheLast miss request should build"),
        )
        .await
        .expect("strict books/list release-date isInTheLast miss request should complete");
    assert_eq!(is_in_the_last_miss.status(), StatusCode::OK);
    let is_in_the_last_miss_payload = response_json(is_in_the_last_miss).await;
    let is_in_the_last_miss_content = is_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date isInTheLast miss payload should expose content array");
    assert_eq!(is_in_the_last_miss_content.len(), 0);

    let is_not_in_the_last_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNotInTheLast",
                            "duration": "P1D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isNotInTheLast match request should build"),
        )
        .await
        .expect("strict books/list release-date isNotInTheLast match request should complete");
    assert_eq!(is_not_in_the_last_match.status(), StatusCode::OK);
    let is_not_in_the_last_match_payload = response_json(is_not_in_the_last_match).await;
    let is_not_in_the_last_match_content = is_not_in_the_last_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date isNotInTheLast match payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_match_content.len(), 1);

    let is_not_in_the_last_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "isNotInTheLast",
                            "duration": "P10000D"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date isNotInTheLast miss request should build"),
        )
        .await
        .expect("strict books/list release-date isNotInTheLast miss request should complete");
    assert_eq!(is_not_in_the_last_miss.status(), StatusCode::OK);
    let is_not_in_the_last_miss_payload = response_json(is_not_in_the_last_miss).await;
    let is_not_in_the_last_miss_content = is_not_in_the_last_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date isNotInTheLast miss payload should expose content array",
        );
    assert_eq!(is_not_in_the_last_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_number_sort_ops_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-list-strict-number-sort").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let number_sort_is_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "is", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort is match request should build"),
        )
        .await
        .expect("strict books/list number-sort is match request should complete");
    assert_eq!(number_sort_is_match.status(), StatusCode::OK);
    let number_sort_is_match_payload = response_json(number_sort_is_match).await;
    let number_sort_is_match_content = number_sort_is_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort is match payload should expose content array");
    assert_eq!(number_sort_is_match_content.len(), 1);

    let number_sort_is_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "is", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort is miss request should build"),
        )
        .await
        .expect("strict books/list number-sort is miss request should complete");
    assert_eq!(number_sort_is_miss.status(), StatusCode::OK);
    let number_sort_is_miss_payload = response_json(number_sort_is_miss).await;
    let number_sort_is_miss_content = number_sort_is_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort is miss payload should expose content array");
    assert_eq!(number_sort_is_miss_content.len(), 0);

    let number_sort_is_not_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "isNot", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort isNot match request should build"),
        )
        .await
        .expect("strict books/list number-sort isNot match request should complete");
    assert_eq!(number_sort_is_not_match.status(), StatusCode::OK);
    let number_sort_is_not_match_payload = response_json(number_sort_is_not_match).await;
    let number_sort_is_not_match_content = number_sort_is_not_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort isNot match payload should expose content array");
    assert_eq!(number_sort_is_not_match_content.len(), 1);

    let number_sort_is_not_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "isNot", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort isNot miss request should build"),
        )
        .await
        .expect("strict books/list number-sort isNot miss request should complete");
    assert_eq!(number_sort_is_not_miss.status(), StatusCode::OK);
    let number_sort_is_not_miss_payload = response_json(number_sort_is_not_miss).await;
    let number_sort_is_not_miss_content = number_sort_is_not_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort isNot miss payload should expose content array");
    assert_eq!(number_sort_is_not_miss_content.len(), 0);

    let number_sort_gt_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "greaterThan", "value": 0.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort greaterThan match request should build"),
        )
        .await
        .expect("strict books/list number-sort greaterThan match request should complete");
    assert_eq!(number_sort_gt_match.status(), StatusCode::OK);
    let number_sort_gt_match_payload = response_json(number_sort_gt_match).await;
    let number_sort_gt_match_content = number_sort_gt_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort greaterThan match payload should expose content array");
    assert_eq!(number_sort_gt_match_content.len(), 1);

    let number_sort_gt_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "greaterThan", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort greaterThan miss request should build"),
        )
        .await
        .expect("strict books/list number-sort greaterThan miss request should complete");
    assert_eq!(number_sort_gt_miss.status(), StatusCode::OK);
    let number_sort_gt_miss_payload = response_json(number_sort_gt_miss).await;
    let number_sort_gt_miss_content = number_sort_gt_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort greaterThan miss payload should expose content array");
    assert_eq!(number_sort_gt_miss_content.len(), 0);

    let number_sort_lt_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "lessThan", "value": 2.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort lessThan match request should build"),
        )
        .await
        .expect("strict books/list number-sort lessThan match request should complete");
    assert_eq!(number_sort_lt_match.status(), StatusCode::OK);
    let number_sort_lt_match_payload = response_json(number_sort_lt_match).await;
    let number_sort_lt_match_content = number_sort_lt_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort lessThan match payload should expose content array");
    assert_eq!(number_sort_lt_match_content.len(), 1);

    let number_sort_lt_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "NumberSort", "operator": "lessThan", "value": 1.0}})
                        .to_string(),
                ))
                .expect("strict books/list number-sort lessThan miss request should build"),
        )
        .await
        .expect("strict books/list number-sort lessThan miss request should complete");
    assert_eq!(number_sort_lt_miss.status(), StatusCode::OK);
    let number_sort_lt_miss_payload = response_json(number_sort_lt_miss).await;
    let number_sort_lt_miss_content = number_sort_lt_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books number-sort lessThan miss payload should expose content array");
    assert_eq!(number_sort_lt_miss_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_tag_author_media_profile_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-tag-author-media-profile").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let tag_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "is", "value": "favorite-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag match request should build"),
        )
        .await
        .expect("strict books/list tag match request should complete");
    assert_eq!(tag_match.status(), StatusCode::OK);
    let tag_match_payload = response_json(tag_match).await;
    let tag_match_content = tag_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag match payload should expose content array");
    assert_eq!(tag_match_content.len(), 1);

    let tag_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "is", "value": "missing-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag miss request should build"),
        )
        .await
        .expect("strict books/list tag miss request should complete");
    assert_eq!(tag_miss.status(), StatusCode::OK);
    let tag_miss_payload = response_json(tag_miss).await;
    let tag_miss_content = tag_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag miss payload should expose content array");
    assert_eq!(tag_miss_content.len(), 0);

    let tag_is_not = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNot", "value": "favorite-tag"}})
                        .to_string(),
                ))
                .expect("strict books/list tag isNot request should build"),
        )
        .await
        .expect("strict books/list tag isNot request should complete");
    assert_eq!(tag_is_not.status(), StatusCode::OK);
    let tag_is_not_payload = response_json(tag_is_not).await;
    let tag_is_not_content = tag_is_not_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNot payload should expose content array");
    assert_eq!(tag_is_not_content.len(), 0);

    let tag_is_null = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNull"}}).to_string(),
                ))
                .expect("strict books/list tag isNull request should build"),
        )
        .await
        .expect("strict books/list tag isNull request should complete");
    assert_eq!(tag_is_null.status(), StatusCode::OK);
    let tag_is_null_payload = response_json(tag_is_null).await;
    let tag_is_null_content = tag_is_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNull payload should expose content array");
    assert_eq!(tag_is_null_content.len(), 0);

    let tag_is_not_null = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Tag", "operator": "isNotNull"}}).to_string(),
                ))
                .expect("strict books/list tag isNotNull request should build"),
        )
        .await
        .expect("strict books/list tag isNotNull request should complete");
    assert_eq!(tag_is_not_null.status(), StatusCode::OK);
    let tag_is_not_null_payload = response_json(tag_is_not_null).await;
    let tag_is_not_null_content = tag_is_not_null_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books tag isNotNull payload should expose content array");
    assert_eq!(tag_is_not_null_content.len(), 1);

    let author_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "jane"}})
                        .to_string(),
                ))
                .expect("strict books/list author match request should build"),
        )
        .await
        .expect("strict books/list author match request should complete");
    assert_eq!(author_match.status(), StatusCode::OK);
    let author_match_payload = response_json(author_match).await;
    let author_match_content = author_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author match payload should expose content array");
    assert_eq!(author_match_content.len(), 1);

    let author_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "Author", "operator": "contains", "value": "missing"}})
                        .to_string(),
                ))
                .expect("strict books/list author miss request should build"),
        )
        .await
        .expect("strict books/list author miss request should complete");
    assert_eq!(author_miss.status(), StatusCode::OK);
    let author_miss_payload = response_json(author_miss).await;
    let author_miss_content = author_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author miss payload should expose content array");
    assert_eq!(author_miss_content.len(), 0);

    let author_role_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "Jane Writer",
                                "role": "writer"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list author role match request should build"),
        )
        .await
        .expect("strict books/list author role match request should complete");
    assert_eq!(author_role_match.status(), StatusCode::OK);
    let author_role_match_payload = response_json(author_role_match).await;
    let author_role_match_content = author_role_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author role match payload should expose content array");
    assert_eq!(author_role_match_content.len(), 1);

    let author_role_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Author",
                            "operator": "is",
                            "value": {
                                "name": "Jane Writer",
                                "role": "editor"
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list author role miss request should build"),
        )
        .await
        .expect("strict books/list author role miss request should complete");
    assert_eq!(author_role_miss.status(), StatusCode::OK);
    let author_role_miss_payload = response_json(author_role_miss).await;
    let author_role_miss_content = author_role_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books author role miss payload should expose content array");
    assert_eq!(author_role_miss_content.len(), 0);

    let poster_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Poster",
                            "operator": "is",
                            "value": {
                                "type": "USER_UPLOADED",
                                "selected": true
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list poster match request should build"),
        )
        .await
        .expect("strict books/list poster match request should complete");
    assert_eq!(poster_match.status(), StatusCode::OK);
    let poster_match_payload = response_json(poster_match).await;
    let poster_match_content = poster_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books poster match payload should expose content array");
    assert_eq!(poster_match_content.len(), 1);

    let poster_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Poster",
                            "operator": "isNot",
                            "value": {
                                "type": "USER_UPLOADED",
                                "selected": true
                            }
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list poster excluded request should build"),
        )
        .await
        .expect("strict books/list poster excluded request should complete");
    assert_eq!(poster_excluded.status(), StatusCode::OK);
    let poster_excluded_payload = response_json(poster_excluded).await;
    let poster_excluded_content = poster_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books poster excluded payload should expose content array");
    assert_eq!(poster_excluded_content.len(), 0);

    let media_profile_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "is", "value": "epub"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile match request should build"),
        )
        .await
        .expect("strict books/list media profile match request should complete");
    assert_eq!(media_profile_match.status(), StatusCode::OK);
    let media_profile_match_payload = response_json(media_profile_match).await;
    let media_profile_match_content = media_profile_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile match payload should expose content array");
    assert_eq!(media_profile_match_content.len(), 1);

    let media_profile_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "is", "value": "pdf"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile miss request should build"),
        )
        .await
        .expect("strict books/list media profile miss request should complete");
    assert_eq!(media_profile_miss.status(), StatusCode::OK);
    let media_profile_miss_payload = response_json(media_profile_miss).await;
    let media_profile_miss_content = media_profile_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile miss payload should expose content array");
    assert_eq!(media_profile_miss_content.len(), 0);

    let media_profile_is_not_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "isNot", "value": "epub"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile isNot excluded request should build"),
        )
        .await
        .expect("strict books/list media profile isNot excluded request should complete");
    assert_eq!(media_profile_is_not_excluded.status(), StatusCode::OK);
    let media_profile_is_not_excluded_payload = response_json(media_profile_is_not_excluded).await;
    let media_profile_is_not_excluded_content = media_profile_is_not_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile isNot excluded payload should expose content array");
    assert_eq!(media_profile_is_not_excluded_content.len(), 0);

    let media_profile_is_not_kept = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"condition": {"type": "MediaProfile", "operator": "isNot", "value": "pdf"}})
                        .to_string(),
                ))
                .expect("strict books/list media profile isNot kept request should build"),
        )
        .await
        .expect("strict books/list media profile isNot kept request should complete");
    assert_eq!(media_profile_is_not_kept.status(), StatusCode::OK);
    let media_profile_is_not_kept_payload = response_json(media_profile_is_not_kept).await;
    let media_profile_is_not_kept_content = media_profile_is_not_kept_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books media profile isNot kept payload should expose content array");
    assert_eq!(media_profile_is_not_kept_content.len(), 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_tag_nullable_operators_with_null_rows_in_runtime_owned_mode()
 {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-tag-nullable-positive").await;
    seed_router_contract_data(&paths).await;
    seed_router_contract_nullable_samples(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for (operator, expected_id) in [
        ("is", "book-1"),
        ("isNot", "book-2"),
        ("isNull", "book-2"),
        ("isNotNull", "book-1"),
    ] {
        let body = if operator == "is" || operator == "isNot" {
            json!({
                "condition": {
                    "type": "Tag",
                    "operator": operator,
                    "value": "favorite-tag",
                }
            })
            .to_string()
        } else {
            json!({
                "condition": {
                    "type": "Tag",
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
                    .uri("/api/v1/books/list?page=0&size=20")
                    .header("x-auth-token", &auth_token)
                    .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body))
                    .expect("strict books/list nullable tag request should build"),
            )
            .await
            .expect("strict books/list nullable tag request should complete");
        assert_eq!(response.status(), StatusCode::OK);
        let payload = response_json(response).await;
        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("strict books nullable tag payload should expose content array");
        assert_eq!(
            content.len(),
            1,
            "unexpected books nullable tag count for operator={operator}",
        );
        assert_eq!(
            content[0].get("id"),
            Some(&Value::String(expected_id.to_string())),
            "unexpected books nullable tag id for operator={operator}",
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_latest_supports_sort_in_runtime_owned_mode() {
    let paths = new_router_fixture("router-discovery-books-latest-strict-sort").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-2", "book-2.cbz", "Another Book").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/latest?page=0&size=20&sort=metadata.title,asc")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .body(Body::empty())
                .expect("strict books/latest sort request should build"),
        )
        .await
        .expect("strict books/latest sort request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let content = payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books/latest sort payload should expose content array");
    assert_eq!(content.len(), 2);
    assert_eq!(content[0].get("id"), Some(&json!("book-2")));
    assert_eq!(content[1].get("id"), Some(&json!("book-1")));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_supports_release_date_string_ops_in_runtime_owned_mode() {
    let paths =
        new_router_fixture("router-discovery-books-list-strict-release-date-string-ops").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let begins_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "beginsWith",
                            "value": "2024-01"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date beginsWith match request should build"),
        )
        .await
        .expect("strict books/list release-date beginsWith match request should complete");
    assert_eq!(begins_with_match.status(), StatusCode::OK);
    let begins_with_match_payload = response_json(begins_with_match).await;
    let begins_with_match_content = begins_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date beginsWith match payload should expose content array");
    assert_eq!(begins_with_match_content.len(), 1);

    let begins_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "beginsWith",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date beginsWith miss request should build"),
        )
        .await
        .expect("strict books/list release-date beginsWith miss request should complete");
    assert_eq!(begins_with_miss.status(), StatusCode::OK);
    let begins_with_miss_payload = response_json(begins_with_miss).await;
    let begins_with_miss_content = begins_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date beginsWith miss payload should expose content array");
    assert_eq!(begins_with_miss_content.len(), 0);

    let ends_with_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "endsWith",
                            "value": "-15"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date endsWith match request should build"),
        )
        .await
        .expect("strict books/list release-date endsWith match request should complete");
    assert_eq!(ends_with_match.status(), StatusCode::OK);
    let ends_with_match_payload = response_json(ends_with_match).await;
    let ends_with_match_content = ends_with_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date endsWith match payload should expose content array");
    assert_eq!(ends_with_match_content.len(), 1);

    let ends_with_miss = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "endsWith",
                            "value": "-99"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date endsWith miss request should build"),
        )
        .await
        .expect("strict books/list release-date endsWith miss request should complete");
    assert_eq!(ends_with_miss.status(), StatusCode::OK);
    let ends_with_miss_payload = response_json(ends_with_miss).await;
    let ends_with_miss_content = ends_with_miss_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("strict books release-date endsWith miss payload should expose content array");
    assert_eq!(ends_with_miss_content.len(), 0);

    let does_not_contain_match = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotContain",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date doesNotContain keep request should build"),
        )
        .await
        .expect("strict books/list release-date doesNotContain keep request should complete");
    assert_eq!(does_not_contain_match.status(), StatusCode::OK);
    let does_not_contain_match_payload = response_json(does_not_contain_match).await;
    let does_not_contain_match_content = does_not_contain_match_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotContain keep payload should expose content array",
        );
    assert_eq!(does_not_contain_match_content.len(), 1);

    let does_not_contain_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotContain",
                            "value": "2024"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotContain excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotContain excluded request should complete");
    assert_eq!(does_not_contain_excluded.status(), StatusCode::OK);
    let does_not_contain_excluded_payload = response_json(does_not_contain_excluded).await;
    let does_not_contain_excluded_content = does_not_contain_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotContain excluded payload should expose content array",
        );
    assert_eq!(does_not_contain_excluded_content.len(), 0);

    let does_not_begin_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotBeginWith",
                            "value": "2025"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotBeginWith keep request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotBeginWith keep request should complete");
    assert_eq!(does_not_begin_with_keep.status(), StatusCode::OK);
    let does_not_begin_with_keep_payload = response_json(does_not_begin_with_keep).await;
    let does_not_begin_with_keep_content = does_not_begin_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotBeginWith keep payload should expose content array",
        );
    assert_eq!(does_not_begin_with_keep_content.len(), 1);

    let does_not_begin_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotBeginWith",
                            "value": "2024"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotBeginWith excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotBeginWith excluded request should complete");
    assert_eq!(does_not_begin_with_excluded.status(), StatusCode::OK);
    let does_not_begin_with_excluded_payload = response_json(does_not_begin_with_excluded).await;
    let does_not_begin_with_excluded_content = does_not_begin_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotBeginWith excluded payload should expose content array",
        );
    assert_eq!(does_not_begin_with_excluded_content.len(), 0);

    let does_not_end_with_keep = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotEndWith",
                            "value": "-99"
                        }
                    })
                    .to_string(),
                ))
                .expect("strict books/list release-date doesNotEndWith keep request should build"),
        )
        .await
        .expect("strict books/list release-date doesNotEndWith keep request should complete");
    assert_eq!(does_not_end_with_keep.status(), StatusCode::OK);
    let does_not_end_with_keep_payload = response_json(does_not_end_with_keep).await;
    let does_not_end_with_keep_content = does_not_end_with_keep_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotEndWith keep payload should expose content array",
        );
    assert_eq!(does_not_end_with_keep_content.len(), 1);

    let does_not_end_with_excluded = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "ReleaseDate",
                            "operator": "doesNotEndWith",
                            "value": "-15"
                        }
                    })
                    .to_string(),
                ))
                .expect(
                    "strict books/list release-date doesNotEndWith excluded request should build",
                ),
        )
        .await
        .expect("strict books/list release-date doesNotEndWith excluded request should complete");
    assert_eq!(does_not_end_with_excluded.status(), StatusCode::OK);
    let does_not_end_with_excluded_payload = response_json(does_not_end_with_excluded).await;
    let does_not_end_with_excluded_content = does_not_end_with_excluded_payload
        .get("content")
        .and_then(Value::as_array)
        .expect(
            "strict books release-date doesNotEndWith excluded payload should expose content array",
        );
    assert_eq!(does_not_end_with_excluded_content.len(), 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_kobo_state_update_roundtrip_persists_progress() {
    let paths = new_router_fixture("router-kobo-state-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "ReadingStates": [{
                            "EntitlementId": "book-1",
                            "LastModified": "2026-03-27T10:00:00Z",
                            "Statistics": {
                                "LastModified": "2026-03-27T10:00:00Z"
                            },
                            "StatusInfo": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "Status": "Reading"
                            },
                            "CurrentBookmark": {
                                "LastModified": "2026-03-27T10:00:00Z",
                                "ProgressPercent": 47.0,
                                "ContentSourceProgressPercent": 23.0,
                                "Location": {
                                    "Source": "/book-1/manifest#position=5",
                                    "Value": "kobo.5.1"
                                }
                            }
                        }]
                    })
                    .to_string(),
                ))
                .expect("kobo state update request should build"),
        )
        .await
        .expect("kobo state update request should complete");
    assert_eq!(put_response.status(), StatusCode::OK);

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/kobo/any-token/v1/library/book-1/state")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("kobo state get request should build"),
        )
        .await
        .expect("kobo state get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let payload = response_json(get_response).await;
    let state = payload
        .as_array()
        .and_then(|values| values.first())
        .expect("kobo state response should contain one reading state object");
    assert_eq!(
        state
            .get("StatusInfo")
            .and_then(|value| value.get("Status")),
        Some(&Value::String("Reading".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ProgressPercent")),
        Some(&json!(47.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("ContentSourceProgressPercent")),
        Some(&json!(23.0)),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Source")),
        Some(&Value::String("/book-1/manifest#position=5".to_string())),
    );
    assert_eq!(
        state
            .get("CurrentBookmark")
            .and_then(|value| value.get("Location"))
            .and_then(|value| value.get("Value")),
        Some(&Value::String("kobo.5.1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_wildcard_routes_match_api_v1_and_opds_v2() {
    let paths = new_router_fixture("router-book-file-wildcard-routes").await;
    seed_router_contract_data(&paths).await;
    let books_dir = paths.config_dir.join("books");
    std::fs::create_dir_all(&books_dir)
        .expect("books directory should be created for file route test");
    let expected_body = b"router-book-file-content";
    std::fs::write(books_dir.join("book-1.epub"), expected_body)
        .expect("book fixture file should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-1/file/book-1.epub",
        "/opds/v2/books/book-1/file/book-1.epub",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("book file wildcard request should build"),
            )
            .await
            .expect("book file wildcard request should complete");

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("book file wildcard response body should be readable");
        assert_eq!(body.as_ref(), expected_body);
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_wildcard_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-book-file-wildcard-missing-file").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/file/book-1.epub")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing wildcard book file request should build"),
        )
        .await
        .expect("missing wildcard book file request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_progress_put_then_get_roundtrip() {
    let paths = new_router_fixture("router-koreader-progress-roundtrip").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let put_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/koreader/syncs/progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "document": "hash-book-1",
                        "percentage": 0.33,
                        "progress": "7",
                        "device": "KOReader",
                        "device_id": "reader-1"
                    })
                    .to_string(),
                ))
                .expect("koreader progress put request should build"),
        )
        .await
        .expect("koreader progress put request should complete");
    assert_eq!(put_response.status(), StatusCode::NO_CONTENT);

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader progress get request should build"),
        )
        .await
        .expect("koreader progress get request should complete");
    assert_eq!(get_response.status(), StatusCode::OK);

    let payload = response_json(get_response).await;
    assert_eq!(
        payload.get("document"),
        Some(&Value::String("hash-book-1".to_string())),
    );
    assert_eq!(
        payload.get("progress"),
        Some(&Value::String("7".to_string()))
    );
    assert_eq!(payload.get("percentage"), Some(&json!(0.33)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_koreader_progress_get_preserves_empty_device_fields() {
    let paths = new_router_fixture("router-koreader-progress-empty-device").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader progress get request should build"),
        )
        .await
        .expect("koreader progress get request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(payload.get("device"), Some(&Value::String(String::new())));
    assert_eq!(
        payload.get("device_id"),
        Some(&Value::String(String::new()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_includes_persisted_authors_tags_and_read_progress() {
    let paths = new_router_fixture("router-discovery-book-detail-persisted-metadata").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, true).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("authors"))
            .and_then(Value::as_array)
            .and_then(|authors| authors.first())
            .and_then(|author| author.get("name")),
        Some(&Value::String("Jane Writer".to_string())),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("authors"))
            .and_then(Value::as_array)
            .and_then(|authors| authors.first())
            .and_then(|author| author.get("role")),
        Some(&Value::String("writer".to_string())),
    );
    assert_eq!(
        payload
            .get("metadata")
            .and_then(|metadata| metadata.get("tags"))
            .and_then(Value::as_array)
            .and_then(|tags| tags.first()),
        Some(&Value::String("favorite-tag".to_string())),
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("page")),
        Some(&json!(10)),
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("completed")),
        Some(&Value::Bool(true)),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_preserves_empty_read_progress_device_fields() {
    let paths = new_router_fixture("router-discovery-book-detail-empty-read-progress-device").await;
    seed_router_contract_data(&paths).await;
    seed_router_read_progress(&paths, false).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book detail device parity db should open");
    sqlx::query(
        "UPDATE READ_PROGRESS SET DEVICE_ID = '', DEVICE_NAME = '' WHERE BOOK_ID = ? AND USER_ID = ?",
    )
    .bind("book-1")
    .bind("admin-user")
    .execute(&pool)
    .await
    .expect("read progress device fields should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceId")),
        Some(&Value::String(String::new()))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|progress| progress.get("deviceName")),
        Some(&Value::String(String::new()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_converts_admin_url_to_file_path() {
    let paths = new_router_fixture("router-discovery-book-detail-admin-url-path").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book detail url parity db should open");
    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind("file:/library%20root/books/book%201.cbz")
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("book url should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("url"),
        Some(&Value::String("/library root/books/book 1.cbz".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_formats_file_last_modified_as_utc_timestamp() {
    let paths = new_router_fixture("router-discovery-book-detail-file-last-modified-utc").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail request should build"),
        )
        .await
        .expect("book detail request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    assert_eq!(
        payload.get("fileLastModified"),
        Some(&Value::String("1970-01-01T00:00:00Z".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_detail_does_not_bridge_missing_book_n_ids() {
    let paths = new_router_fixture("router-discovery-book-detail-no-bridge-id").await;
    seed_router_contract_data(&paths).await;
    seed_router_cbz_book(&paths, "book-z-2", "book-z-2.cbz", "Second Real Book").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail bridge-id request should build"),
        )
        .await
        .expect("book detail bridge-id request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_metadata_batch_update_persists_title_and_updates_book_snapshot() {
    let paths =
        new_router_fixture("router-book-metadata-batch-update-persists-and-touches-book").await;
    seed_router_contract_data(&paths).await;

    let pool_before = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open before metadata batch update");
    let last_modified_before = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM BOOK WHERE ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&pool_before)
    .await
    .expect("book last modified should be queryable before metadata batch update")
    .get::<String, _>("LAST_MODIFIED");
    pool_before.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let patch = json!({
        "book-1": {
            "title": "Updated Batch Title"
        }
    });

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch.to_string()))
                .expect("book metadata batch update request should build"),
        )
        .await
        .expect("book metadata batch update request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after metadata batch update request should build"),
        )
        .await
        .expect("book detail after metadata batch update request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload.get("metadata").and_then(|value| value.get("title")),
        Some(&Value::String("Updated Batch Title".to_string()))
    );

    let pool_after = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open after metadata batch update");
    let last_modified_after = sqlx::query(
        "SELECT COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED FROM BOOK WHERE ID = ? LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&pool_after)
    .await
    .expect("book last modified should be queryable after metadata batch update")
    .get::<String, _>("LAST_MODIFIED");
    pool_after.close().await;
    assert_ne!(last_modified_after, last_modified_before);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_metadata_batch_update_refreshes_book_search_results() {
    let paths = new_router_fixture("router-book-metadata-batch-update-refreshes-search").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let initial_search = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "is",
                            "value": "Book 1"
                        }
                    })
                    .to_string(),
                ))
                .expect("initial books/list title search request should build"),
        )
        .await
        .expect("initial books/list title search request should complete");
    assert_eq!(initial_search.status(), StatusCode::OK);
    let initial_payload = response_json(initial_search).await;
    let initial_content = initial_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("initial books/list title search should expose content array");
    assert_eq!(initial_content.len(), 1);
    assert_eq!(
        initial_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    let patch = json!({
        "book-1": {
            "title": "Updated Batch Title"
        }
    });
    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/metadata")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(patch.to_string()))
                .expect("book metadata batch update request should build"),
        )
        .await
        .expect("book metadata batch update request should complete");
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let updated_search = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "is",
                            "value": "Updated Batch Title"
                        }
                    })
                    .to_string(),
                ))
                .expect("updated books/list title search request should build"),
        )
        .await
        .expect("updated books/list title search request should complete");
    assert_eq!(updated_search.status(), StatusCode::OK);
    let updated_payload = response_json(updated_search).await;
    let updated_content = updated_payload
        .get("content")
        .and_then(Value::as_array)
        .expect("updated books/list title search should expose content array");
    assert_eq!(updated_content.len(), 1);
    assert_eq!(
        updated_content[0].get("id"),
        Some(&Value::String("book-1".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_book_readlists_returns_existing_persisted_readlists() {
    let paths = new_router_fixture("router-discovery-book-readlists-persisted").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/readlists")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book readlists request should build"),
        )
        .await
        .expect("book readlists request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    let content = payload
        .as_array()
        .expect("book readlists payload should be an array");
    assert_eq!(content.len(), 1);
    assert_eq!(
        content[0].get("id"),
        Some(&Value::String("readlist-1".to_string())),
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_pages_and_raw_pages_include_inline_content_disposition() {
    let paths = new_router_fixture("router-book-pages-inline-disposition").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    update_router_book_name(&paths, "book-pdf-1", "Readable Page Title").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-pdf-1/pages/1",
        "/api/v1/books/book-pdf-1/pages/1/raw",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("page request should build"),
            )
            .await
            .expect("page request should complete");

        assert_eq!(response.status(), StatusCode::OK, "route: {route}");
        let disposition = response
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("page response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("Readable Page Title-1"),
            "route: {route}, disposition: {disposition}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_does_not_return_not_modified_before_page_streaming_check() {
    let paths = new_router_fixture("router-book-raw-page-no-304-before-role-check").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "admin-access-no-page-streaming",
        "admin-access-no-page-streaming@example.org",
        "router-contract-admin-access-123",
        0,
        &["USER", "ADMIN"],
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "admin-access-no-page-streaming@example.org",
        "router-contract-admin-access-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 01 Jan 2099 00:00:00 +0000")
                .body(Body::empty())
                .expect("book raw page role-order request should build"),
        )
        .await
        .expect("book raw page role-order request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_negative_page_number() {
    let paths = new_router_fixture("router-book-raw-page-negative-page-number").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/-1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("negative raw page request should build"),
        )
        .await
        .expect("negative raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_non_integer_page_number() {
    let paths = new_router_fixture("router-book-raw-page-non-integer-page-number").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/abc/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("non-integer raw page request should build"),
        )
        .await
        .expect("non-integer raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_content_disposition_uses_book_name_not_metadata_title() {
    let paths = new_router_fixture("router-book-raw-page-uses-book-name").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Metadata Title",
    )
    .await;
    update_router_book_name(&paths, "book-pdf-1", "Filesystem Shelf Name").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book raw page request should build"),
        )
        .await
        .expect("book raw page request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .expect("raw page response should expose content-disposition");
    assert!(
        disposition.contains("Filesystem Shelf Name-1"),
        "disposition was: {disposition}"
    );
    assert!(
        !disposition.contains("Metadata Title-1"),
        "disposition was: {disposition}"
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_for_missing_pdf_page_number() {
    let paths = new_router_fixture("router-book-page-missing-pdf-page").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/2/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing raw pdf page request should build"),
        )
        .await
        .expect("missing raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_found_with_message_when_media_not_ready() {
    let paths = new_router_fixture("router-book-raw-page-media-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page not-ready db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("not-ready raw pdf page request should build"),
        )
        .await
        .expect("not-ready raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Book analysis failed".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_forbidden_before_not_ready_for_restricted_user() {
    let paths = new_router_fixture("router-book-raw-page-restricted-before-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-page-user",
        "restricted-page-user@example.org",
        "router-contract-restricted-page-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page restricted db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    sqlx::query("UPDATE SERIES_METADATA SET AGE_RATING = ? WHERE SERIES_ID = ?")
        .bind(18_i64)
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series age rating should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted-page-user@example.org",
        "router-contract-restricted-page-123",
    )
    .await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("restricted raw pdf page request should build"),
        )
        .await
        .expect("restricted raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::FORBIDDEN);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_found_with_message_when_file_is_missing() {
    let paths = new_router_fixture("router-book-raw-page-file-missing").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pdf_path = paths.config_dir.join("books/fixture-page.pdf");
    std::fs::remove_file(&pdf_path).expect("pdf fixture should be removable");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("file-missing raw pdf page request should build"),
        )
        .await
        .expect("file-missing raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "File not found, it may have moved".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_not_modified_before_not_ready_checks() {
    let paths = new_router_fixture("router-book-raw-page-not-modified-before-not-ready").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book raw page not-ready db should open");
    sqlx::query("UPDATE MEDIA SET STATUS = ? WHERE BOOK_ID = ?")
        .bind("OUTDATED")
        .bind("book-pdf-1")
        .execute(&pool)
        .await
        .expect("media status should update");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 01 Jan 2099 00:00:00 +0000")
                .body(Body::empty())
                .expect("not-modified raw pdf page request should build"),
        )
        .await
        .expect("not-modified raw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::NOT_MODIFIED);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_resource_supports_not_modified_and_inline_content_disposition() {
    let paths = new_router_fixture("router-book-resource-inline-not-modified").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><p>Hello</p></body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for route in [
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/opds/v2/books/book-1/resource/OEBPS/chapter.xhtml",
    ] {
        let initial = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("resource request should build"),
            )
            .await
            .expect("resource request should complete");

        assert_eq!(initial.status(), StatusCode::OK, "route: {route}");
        let last_modified = initial
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose last-modified")
            .to_string();
        let disposition = initial
            .headers()
            .get(header::CONTENT_DISPOSITION)
            .and_then(|value| value.to_str().ok())
            .expect("resource response should expose content-disposition");
        assert!(
            disposition.starts_with("inline;"),
            "route: {route}, disposition: {disposition}"
        );
        assert!(
            disposition.contains("chapter.xhtml"),
            "route: {route}, disposition: {disposition}"
        );
        assert_eq!(
            initial
                .headers()
                .get(header::CONTENT_SECURITY_POLICY)
                .and_then(|value| value.to_str().ok()),
            Some("script-src 'none'; object-src 'none';"),
            "route: {route}"
        );

        let not_modified = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::IF_MODIFIED_SINCE, &last_modified)
                    .body(Body::empty())
                    .expect("conditional resource request should build"),
            )
            .await
            .expect("conditional resource request should complete");

        assert_eq!(
            not_modified.status(),
            StatusCode::NOT_MODIFIED,
            "route: {route}"
        );
        assert_eq!(
            not_modified
                .headers()
                .get(header::LAST_MODIFIED)
                .and_then(|value| value.to_str().ok()),
            Some(last_modified.as_str()),
            "route: {route}"
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_parses_multipart_image_and_selected_flag() {
    let paths = new_router_fixture("router-book-thumbnail-upload-multipart").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

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
    let payload = response_json(upload).await;
    assert_eq!(
        payload.get("bookId"),
        Some(&Value::String("book-1".to_string()))
    );
    assert_eq!(
        payload.get("type"),
        Some(&Value::String("USER_UPLOADED".to_string()))
    );
    assert!(
        payload.get("id").and_then(Value::as_str).is_some(),
        "book thumbnail upload should return thumbnail id"
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

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_selects_thumbnail_when_none_was_selected() {
    let paths = new_router_fixture("router-book-thumbnail-upload-auto-selects-first").await;
    seed_router_contract_data(&paths).await;

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before upload test");
    cleanup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

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

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for selected thumbnail verification");
    let selected_count = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND SELECTED = 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("selected book thumbnails should be queryable")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;

    assert_eq!(selected_count, 1);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_by_id_allows_missing_path_book_when_thumbnail_exists() {
    let paths = new_router_fixture("router-book-thumbnail-by-id-missing-path-book").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

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

    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("book thumbnail upload should return thumbnail id")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/v1/books/missing-book/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail missing path request should build"),
        )
        .await
        .expect("book thumbnail missing path request should complete");

    assert_eq!(response.status(), StatusCode::OK);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_delete_allows_missing_path_book_when_thumbnail_exists() {
    let paths = new_router_fixture("router-book-thumbnail-delete-missing-path-book").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let (content_type, body) =
        multipart_image_upload_body("file", "cover.png", "image/png", false, &image_bytes);

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
    let thumbnail_id = response_json(upload)
        .await
        .get("id")
        .and_then(Value::as_str)
        .expect("book thumbnail upload should return thumbnail id")
        .to_string();

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/books/missing-book/thumbnails/{thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail missing path delete request should build"),
        )
        .await
        .expect("book thumbnail missing path delete request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for missing path delete verification");
    let remaining = sqlx::query("SELECT COUNT(*) AS COUNT FROM THUMBNAIL_BOOK WHERE ID = ?")
        .bind(&thumbnail_id)
        .fetch_one(&verify_pool)
        .await
        .expect("book thumbnail delete should be queryable")
        .get::<i64, _>("COUNT");
    verify_pool.close().await;
    assert_eq!(remaining, 0);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_delete_rejects_generated_thumbnail() {
    let paths = new_router_fixture("router-book-thumbnail-delete-generated").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing thumbnails should be deleted before generated delete test");
    cleanup_pool.close().await;

    generate_book_thumbnail(paths.main_db.as_path(), "book-1")
        .expect("generate_book_thumbnail should succeed before delete test");

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for generated thumbnail lookup");
    let generated_thumbnail_id = sqlx::query(
        "SELECT ID FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? AND TYPE = 'GENERATED' LIMIT 1",
    )
    .bind("book-1")
    .fetch_one(&verify_pool)
    .await
    .expect("generated thumbnail row should be queryable")
    .get::<String, _>("ID");
    verify_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/books/book-1/thumbnails/{generated_thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("generated book thumbnail delete request should build"),
        )
        .await
        .expect("generated book thumbnail delete request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_delete_reselects_remaining_thumbnail_when_selected_one_is_removed() {
    let paths = new_router_fixture("router-book-thumbnail-delete-reselects-remaining").await;
    seed_router_contract_data(&paths).await;

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for book thumbnail delete cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before delete reselect test");
    cleanup_pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();

    let mut selected_thumbnail_id = String::new();
    for (selected, name) in [(true, "selected.png"), (false, "other.png")] {
        let (content_type, body) =
            multipart_image_upload_body("file", name, "image/png", selected, &image_bytes);
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
        let thumbnail_id = response_json(upload)
            .await
            .get("id")
            .and_then(Value::as_str)
            .expect("uploaded book thumbnail should expose id")
            .to_string();
        if selected {
            selected_thumbnail_id = thumbnail_id;
        }
    }

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!(
                    "/api/v1/books/book-1/thumbnails/{selected_thumbnail_id}"
                ))
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book selected thumbnail delete request should build"),
        )
        .await
        .expect("book selected thumbnail delete request should complete");
    assert_eq!(delete.status(), StatusCode::ACCEPTED);

    let list = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnail list request should build"),
        )
        .await
        .expect("book thumbnail list request should complete");
    assert_eq!(list.status(), StatusCode::OK);
    let rows = response_json(list).await;
    let rows = rows
        .as_array()
        .expect("book thumbnail list response should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("selected"), Some(&Value::Bool(true)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_epub_cover() {
    let paths = new_router_fixture("router-generate-book-thumbnail-epub").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let cleanup_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub thumbnail cleanup");
    sqlx::query("DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID = ?")
        .bind("book-1")
        .execute(&cleanup_pool)
        .await
        .expect("existing book-1 thumbnails should be deleted before epub cover test");
    cleanup_pool.close().await;

    let media = BookMediaRecord {
        library_id: "library-1".to_string(),
        media_type: "application/epub+zip".to_string(),
        file_path: paths.config_dir.join("books/book-1.epub"),
        file_name: "book-1.epub".to_string(),
        page_count: 10,
    };
    let (cover_bytes, cover_media_type) =
        load_epub_cover_bytes(&media).expect("epub cover bytes should be extractable");
    assert!(!cover_bytes.is_empty());
    assert_eq!(cover_media_type, "image/png");

    generate_book_thumbnail(paths.main_db.as_path(), "book-1")
        .expect("generate_book_thumbnail should execute successfully for epub cover");

    let main_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub generated thumbnail verification");
    let generated = sqlx::query(
        "SELECT TYPE, MEDIA_TYPE, WIDTH, HEIGHT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC",
    )
    .bind("book-1")
    .fetch_all(&main_pool)
    .await
    .expect("epub generated thumbnail rows should be queryable");
    main_pool.close().await;
    assert_eq!(generated.len(), 1);
    let generated_row = generated
        .iter()
        .find(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .expect("epub generated thumbnail row should exist");
    assert_eq!(generated_row.get::<String, _>("MEDIA_TYPE"), "image/jpeg");

    let runtime_config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&runtime_config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub book thumbnail request should build after generate task"),
        )
        .await
        .expect("epub book thumbnail request should complete after generate task");
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(
        after
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );

    let thumbnails = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("epub book thumbnails request should build after generate task"),
        )
        .await
        .expect("epub book thumbnails request should complete after generate task");
    assert_eq!(thumbnails.status(), StatusCode::OK);
    let payload = response_json(thumbnails).await;
    assert_eq!(
        payload
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("type")),
        Some(&Value::String("GENERATED".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn generate_book_thumbnail_persists_generated_thumbnail_for_pdf() {
    let paths = new_router_fixture("router-generate-book-thumbnail-pdf").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    generate_book_thumbnail(paths.main_db.as_path(), "book-pdf-1")
        .expect("generate_book_thumbnail should execute successfully for pdf");

    let main_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for pdf generated thumbnail verification");
    let generated = sqlx::query(
        "SELECT TYPE, MEDIA_TYPE, WIDTH, HEIGHT FROM THUMBNAIL_BOOK WHERE BOOK_ID = ? ORDER BY ID ASC",
    )
    .bind("book-pdf-1")
    .fetch_all(&main_pool)
    .await
    .expect("pdf generated thumbnail rows should be queryable");
    main_pool.close().await;
    assert_eq!(generated.len(), 1);
    let generated_row = generated
        .iter()
        .find(|row| row.get::<String, _>("TYPE") == "GENERATED")
        .expect("pdf generated thumbnail row should exist");
    assert_eq!(generated_row.get::<String, _>("MEDIA_TYPE"), "image/jpeg");
    assert!(generated_row.get::<i64, _>("WIDTH") > 0);
    assert!(generated_row.get::<i64, _>("HEIGHT") > 0);

    let runtime_config = runtime_config_for_paths(&paths);
    let app = build_router_with_config(&runtime_config);
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let after = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/thumbnail")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf book thumbnail request should build after generate task"),
        )
        .await
        .expect("pdf book thumbnail request should complete after generate task");
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(
        after
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("image/jpeg")
    );

    let thumbnails = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf book thumbnails request should build after generate task"),
        )
        .await
        .expect("pdf book thumbnails request should complete after generate task");
    assert_eq!(thumbnails.status(), StatusCode::OK);
    let payload = response_json(thumbnails).await;
    assert_eq!(
        payload
            .as_array()
            .and_then(|items| items.first())
            .and_then(|item| item.get("type")),
        Some(&Value::String("GENERATED".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnails_returns_empty_array_for_existing_book_without_posters() {
    let paths = new_router_fixture("router-book-thumbnails-empty-array").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-2/thumbnails")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book thumbnails empty request should build"),
        )
        .await
        .expect("book thumbnails empty request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(payload, json!([]));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_media_asset_routes_forbid_age_restricted_user() {
    let paths = new_router_fixture("router-book-media-asset-restricted-user").await;
    seed_router_contract_data(&paths).await;
    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        12,
        &["USER", "PAGE_STREAMING", "FILE_DOWNLOAD"],
    )
    .await;
    write_router_epub_resource(
        &paths,
        "books/book-1.epub",
        "OEBPS/chapter.xhtml",
        br#"<html xmlns='http://www.w3.org/1999/xhtml'><body>Restricted</body></html>"#,
    );

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;

    for route in [
        "/api/v1/books/book-1/file",
        "/api/v1/books/book-1/thumbnails",
        "/api/v1/books/book-1/manifest",
        "/api/v1/books/book-1/resource/OEBPS/chapter.xhtml",
        "/api/v1/books/book-1/progression",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .body(Body::empty())
                    .expect("restricted media asset get request should build"),
            )
            .await
            .expect("restricted media asset get request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    for route in ["/api/v1/books/book-1/progression"] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri(route)
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        json!({
                            "locator": {
                                "locations": {
                                    "progression": 0.25
                                }
                            }
                        })
                        .to_string(),
                    ))
                    .expect("restricted media asset put request should build"),
            )
            .await
            .expect("restricted media asset put request should complete");

        assert_eq!(response.status(), StatusCode::FORBIDDEN, "route: {route}");
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_file_delete_enqueues_delete_book_even_when_book_is_missing() {
    let paths = new_router_fixture("router-book-file-delete-missing-book").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/missing-book/file")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing book file delete request should build"),
        )
        .await
        .expect("missing book file delete request should complete");

    assert_eq!(response.status(), StatusCode::ACCEPTED);

    let tasks_pool = connect_pool(paths.tasks_db.as_path(), 1)
        .await
        .expect("tasks db should open for missing book file delete verification");
    let rows = sqlx::query("SELECT ID, SIMPLE_TYPE, GROUP_ID FROM TASK ORDER BY ID ASC")
        .fetch_all(&tasks_pool)
        .await
        .expect("missing book delete task rows should be queryable");
    tasks_pool.close().await;

    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get::<String, _>("ID"), "DELETE_BOOK:missing-book");
    assert_eq!(rows[0].get::<String, _>("SIMPLE_TYPE"), "DELETE_BOOK");
    assert_eq!(
        rows[0].get::<Option<String>, _>("GROUP_ID"),
        Some("missing-book".to_string())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_thumbnail_upload_rejects_invalid_selected_flag() {
    let paths = new_router_fixture("router-book-thumbnail-upload-invalid-selected").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;
    let image_bytes = fixture_png_bytes();
    let boundary = "komga-rust-invalid-selected-boundary";
    let mut body = Vec::new();
    use std::io::Write as _;
    write!(
        &mut body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"cover.png\"\r\nContent-Type: image/png\r\n\r\n"
    )
    .expect("multipart invalid-selected file prelude should be written");
    body.extend_from_slice(&image_bytes);
    write!(
        &mut body,
        "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"selected\"\r\n\r\nmaybe\r\n--{boundary}--\r\n"
    )
    .expect("multipart invalid-selected field should be written");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/book-1/thumbnails")
                .header("x-auth-token", &auth_token)
                .header(
                    header::CONTENT_TYPE,
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .expect("invalid selected thumbnail upload request should build"),
        )
        .await
        .expect("invalid selected thumbnail upload request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "book thumbnail selected field must be true or false".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_pages_single_image_fallback_includes_dimensions() {
    let paths = new_router_fixture("router-book-pages-single-image-dimensions").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image page fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-1")
    .bind(0_i64)
    .bind("cover.png")
    .bind("books/cover.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(5_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("READY")
        .bind("book-image-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("5")
    .bind(5.0_f64)
    .bind("Cover Book")
    .bind("2024-02-02")
    .bind("book-image-1")
    .execute(&pool)
    .await
    .expect("single-image book metadata row should be inserted");
    pool.close().await;

    let image_path = paths.config_dir.join("books/cover.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent).expect("single-image parent directory should be created");
    }
    std::fs::write(&image_path, fixture_png_bytes())
        .expect("single-image fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image pages request should build"),
        )
        .await
        .expect("single-image pages request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let rows = payload
        .as_array()
        .expect("single-image pages payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].get("width"), Some(&json!(1)));
    assert_eq!(rows[0].get("height"), Some(&json!(1)));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_raw_page_returns_bad_request_with_message_for_non_pdf_media() {
    let paths = new_router_fixture("router-book-raw-page-single-image").await;
    seed_router_contract_data(&paths).await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for single-image raw fixture");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-image-raw-1")
    .bind(0_i64)
    .bind("cover.png")
    .bind("books/cover-raw.png")
    .bind("series-1")
    .bind(1_i64)
    .bind(6_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("single-image raw book row should be inserted");
    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("image/png")
        .bind("READY")
        .bind("book-image-raw-1")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("single-image raw media row should be inserted");
    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("6")
    .bind(6.0_f64)
    .bind("Cover Raw Book")
    .bind("2024-02-03")
    .bind("book-image-raw-1")
    .execute(&pool)
    .await
    .expect("single-image raw metadata row should be inserted");
    pool.close().await;

    let image_path = paths.config_dir.join("books/cover-raw.png");
    if let Some(parent) = image_path.parent() {
        std::fs::create_dir_all(parent)
            .expect("single-image raw parent directory should be created");
    }
    let image_bytes = fixture_png_bytes();
    std::fs::write(&image_path, &image_bytes).expect("single-image raw fixture should be written");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-image-raw-1/pages/1/raw")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("single-image raw page request should build"),
        )
        .await
        .expect("single-image raw page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Extractor does not support raw extraction of pages".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_pages_generated_pdf_fallback_matches_kotlin_page_shape() {
    let paths = new_router_fixture("router-book-pages-pdf-dimensions").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("pdf pages request should build"),
        )
        .await
        .expect("pdf pages request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let rows = payload
        .as_array()
        .expect("pdf pages payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("fileName"),
        Some(&Value::String("1".to_string()))
    );
    assert_eq!(
        rows[0].get("mediaType"),
        Some(&Value::String("image/jpeg".to_string()))
    );
    assert!(rows[0].get("width").is_some_and(|value| !value.is_null()));
    assert!(rows[0].get("height").is_some_and(|value| !value.is_null()));
    assert!(rows[0].get("sizeBytes").is_some_and(Value::is_null));
    assert_eq!(rows[0].get("size"), Some(&Value::String(String::new())));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_returns_bad_request_with_message_for_missing_pdf_page_number() {
    let paths = new_router_fixture("router-book-page-missing-pdf-page-nonraw").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/2")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("missing nonraw pdf page request should build"),
        )
        .await
        .expect("missing nonraw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_page_pdf_negotiation_returns_bad_request_with_message_for_missing_pdf_page_number()
 {
    let paths =
        new_router_fixture("router-book-page-missing-pdf-page-nonraw-pdf-negotiation").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Readable Page Title",
    )
    .await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages/2")
                .header("x-auth-token", &auth_token)
                .header(header::ACCEPT, "application/pdf")
                .body(Body::empty())
                .expect("missing negotiated nonraw pdf page request should build"),
        )
        .await
        .expect("missing negotiated nonraw pdf page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String("Page number does not exist".to_string()))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_requires_page_when_completed_is_false_or_missing() {
    let paths = new_router_fixture("router-book-read-progress-requires-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for body in [json!({}), json!({ "completed": false })] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("book read-progress missing-page request should build"),
            )
            .await
            .expect("book read-progress missing-page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response_json(response).await;
        assert_eq!(payload.get("violations"), Some(&json!([])));
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_treats_null_page_like_missing_page() {
    let paths = new_router_fixture("router-book-read-progress-null-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for body in [
        json!({ "page": Value::Null }),
        json!({ "page": Value::Null, "completed": false }),
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("book read-progress null-page request should build"),
            )
            .await
            .expect("book read-progress null-page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response_json(response).await;
        assert_eq!(payload.get("violations"), Some(&json!([])));
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_rejects_non_positive_page_with_validation_payload() {
    let paths = new_router_fixture("router-book-read-progress-non-positive-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    for body in [json!({ "page": 0 }), json!({ "page": -1 })] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/api/v1/books/book-1/read-progress")
                    .header("x-auth-token", &auth_token)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(body.to_string()))
                    .expect("book read-progress non-positive page request should build"),
            )
            .await
            .expect("book read-progress non-positive page request should complete");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let payload = response_json(response).await;
        assert_eq!(
            payload.get("violations"),
            Some(&json!([
                {
                    "fieldName": "page",
                    "message": "must be greater than 0"
                }
            ]))
        );
    }

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_completed_true_still_rejects_non_positive_page() {
    let paths =
        new_router_fixture("router-book-read-progress-completed-true-non-positive-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "page": 0, "completed": true }).to_string(),
                ))
                .expect("book read-progress completed-true non-positive page request should build"),
        )
        .await
        .expect("book read-progress completed-true non-positive page request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("violations"),
        Some(&json!([
            {
                "fieldName": "page",
                "message": "must be greater than 0"
            }
        ]))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_completed_true_ignores_positive_page_and_marks_completed() {
    let paths = new_router_fixture("router-book-read-progress-completed-true-ignores-page").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "page": 5, "completed": true }).to_string(),
                ))
                .expect("book read-progress completed-true with page request should build"),
        )
        .await
        .expect("book read-progress completed-true with page request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after completed-true with page request should build"),
        )
        .await
        .expect("book detail after completed-true with page request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("page")),
        Some(&Value::from(10))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("completed")),
        Some(&Value::Bool(true))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_completed_true_ignores_out_of_range_positive_page() {
    let paths =
        new_router_fixture("router-book-read-progress-completed-true-ignores-out-of-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({ "page": 999, "completed": true }).to_string(),
                ))
                .expect(
                    "book read-progress completed-true with out-of-range page request should build",
                ),
        )
        .await
        .expect("book read-progress completed-true with out-of-range page request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect(
                    "book detail after completed-true with out-of-range page request should build",
                ),
        )
        .await
        .expect("book detail after completed-true with out-of-range page request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("page")),
        Some(&Value::from(10))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("completed")),
        Some(&Value::Bool(true))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_rejects_page_beyond_page_count_with_specific_error() {
    let paths = new_router_fixture("router-book-read-progress-page-out-of-range").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 999 }).to_string()))
                .expect("book read-progress out-of-range request should build"),
        )
        .await
        .expect("book read-progress out-of-range request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("error"),
        Some(&Value::String(
            "Page argument (999) must be within 1 and book page count (10)".to_string()
        ))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_marks_completed_when_page_equals_last_page() {
    let paths = new_router_fixture("router-book-read-progress-last-page-completes").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 10 }).to_string()))
                .expect("book read-progress last-page request should build"),
        )
        .await
        .expect("book read-progress last-page request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let detail = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after last-page read-progress request should build"),
        )
        .await
        .expect("book detail after last-page read-progress request should complete");

    assert_eq!(detail.status(), StatusCode::OK);
    let payload = response_json(detail).await;
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("page")),
        Some(&Value::from(10))
    );
    assert_eq!(
        payload
            .get("readProgress")
            .and_then(|value| value.get("completed")),
        Some(&Value::Bool(true))
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_persists_epub_locator_for_page_updates() {
    let paths = new_router_fixture("router-book-read-progress-persists-epub-locator").await;
    seed_router_contract_data(&paths).await;

    let positions = json!([
        {
            "href": "/book-1.xhtml#kobo.1.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 1,
                "progression": 0.0,
                "totalProgression": 0.1
            }
        },
        {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            }
        }
    ]);

    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub locator seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for read-progress locator test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 2 }).to_string()))
                .expect("book read-progress epub locator request should build"),
        )
        .await
        .expect("book read-progress epub locator request should complete");

    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub locator verification");
    let locator_row =
        sqlx::query("SELECT LOCATOR FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ? LIMIT 1")
            .bind("book-1")
            .bind("admin-user")
            .fetch_one(&verify_pool)
            .await
            .expect("read progress locator should be queryable");
    let locator_blob = locator_row
        .try_get::<Option<Vec<u8>>, _>("LOCATOR")
        .or_else(|_| locator_row.try_get::<Option<Vec<u8>>, _>("locator"))
        .expect("read progress locator column should be readable");
    verify_pool.close().await;

    let locator = locator_blob.as_deref().map(|blob| {
        serde_json::from_slice::<Value>(blob).expect("locator blob should be valid JSON")
    });
    assert_eq!(
        locator,
        positions.as_array().and_then(|items| items.get(1)).cloned()
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_read_progress_delete_clears_persisted_progress_and_koreader_view() {
    let paths = new_router_fixture("router-book-read-progress-delete-clears-progress").await;
    seed_router_contract_data(&paths).await;
    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for read-progress delete locator seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded for read-progress delete test");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let update = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({ "page": 2 }).to_string()))
                .expect("book read-progress setup request should build"),
        )
        .await
        .expect("book read-progress setup request should complete");
    assert_eq!(update.status(), StatusCode::NO_CONTENT);

    let delete = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/books/book-1/read-progress")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book read-progress delete request should build"),
        )
        .await
        .expect("book read-progress delete request should complete");
    assert_eq!(delete.status(), StatusCode::NO_CONTENT);

    let detail = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book detail after read-progress delete request should build"),
        )
        .await
        .expect("book detail after read-progress delete request should complete");
    assert_eq!(detail.status(), StatusCode::OK);
    let detail_payload = response_json(detail).await;
    assert_eq!(detail_payload.get("readProgress"), Some(&Value::Null));

    let koreader = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/koreader/syncs/progress/hash-book-1")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("koreader progress after delete request should build"),
        )
        .await
        .expect("koreader progress after delete request should complete");
    assert_eq!(koreader.status(), StatusCode::NOT_FOUND);

    let verify_pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for read-progress delete verification");
    let remaining = sqlx::query(
        "SELECT COUNT(*) AS COUNT FROM READ_PROGRESS WHERE BOOK_ID = ? AND USER_ID = ?",
    )
    .bind("book-1")
    .bind("admin-user")
    .fetch_one(&verify_pool)
    .await
    .expect("read-progress delete verification query should succeed")
    .get::<i64, _>("COUNT");
    verify_pool.close().await;
    assert_eq!(remaining, 0);
    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_positions_returns_not_found_without_epub_extension_positions() {
    let paths = new_router_fixture("router-book-positions-no-extension").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book positions request should build"),
        )
        .await
        .expect("book positions request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_positions_does_not_return_not_modified_when_positions_are_missing() {
    let paths = new_router_fixture("router-book-positions-no-extension-not-modified").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, "Wed, 31 Dec 2099 23:59:59 GMT")
                .body(Body::empty())
                .expect("book positions conditional missing request should build"),
        )
        .await
        .expect("book positions conditional missing request should complete");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_progression_get_returns_full_r2_progression_shape() {
    let paths = new_router_fixture("router-book-progression-get-full-shape").await;
    seed_router_contract_data(&paths).await;

    let locator = json!({
        "href": "/book-1.xhtml#kobo.2.1",
        "type": "application/xhtml+xml",
        "title": "Chapter 2",
        "locations": {
            "position": 2,
            "progression": 0.5,
            "totalProgression": 0.2
        },
        "text": {
            "highlight": "Some text"
        },
        "koboSpan": "kobo-span-2"
    });

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for progression shape seed");
    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, DEVICE_ID, DEVICE_NAME, LOCATOR) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(2_i64)
    .bind(false)
    .bind("2024-01-02 03:04:05")
    .bind("reader-1")
    .bind("KOReader")
    .bind(serde_json::to_vec(&locator).expect("locator should serialize"))
    .execute(&pool)
    .await
    .expect("read progress row for progression shape should insert");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/progression")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book progression request should build"),
        )
        .await
        .expect("book progression request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    assert_eq!(
        payload.get("modified"),
        Some(&Value::String("2024-01-02T03:04:05Z".to_string()))
    );
    assert_eq!(
        payload.get("device"),
        Some(&json!({
            "id": "reader-1",
            "name": "KOReader"
        }))
    );
    assert_eq!(payload.get("locator"), Some(&locator));

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_positions_returns_epub_extension_positions_and_supports_not_modified() {
    let paths = new_router_fixture("router-book-positions-epub-extension").await;
    seed_router_contract_data(&paths).await;
    write_router_epub_with_cover(&paths, "books/book-1.epub");

    let positions = json!([
        {
            "href": "/book-1.xhtml#kobo.1.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 1,
                "progression": 0.0,
                "totalProgression": 0.1
            }
        },
        {
            "href": "/book-1.xhtml#kobo.2.1",
            "type": "application/xhtml+xml",
            "locations": {
                "position": 2,
                "progression": 0.5,
                "totalProgression": 0.2
            }
        }
    ]);
    let extension_blob = fixture_epub_positions_extension_blob();

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("main db should open for epub extension positions seed");
    sqlx::query("UPDATE MEDIA SET EXTENSION_CLASS = ?, EXTENSION_VALUE_BLOB = ? WHERE BOOK_ID = ?")
        .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
        .bind(extension_blob)
        .bind("book-1")
        .execute(&pool)
        .await
        .expect("epub extension positions should be seeded");
    pool.close().await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let initial = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("book positions initial request should build"),
        )
        .await
        .expect("book positions initial request should complete");

    assert_eq!(initial.status(), StatusCode::OK);
    assert_eq!(
        initial
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok()),
        Some("application/vnd.readium.position-list+json")
    );
    let last_modified = initial
        .headers()
        .get(header::LAST_MODIFIED)
        .and_then(|value| value.to_str().ok())
        .expect("book positions response should expose last-modified")
        .to_string();
    let payload = response_json(initial).await;
    assert_eq!(payload.get("total"), Some(&Value::from(2)));
    assert_eq!(payload.get("positions"), Some(&positions));

    let not_modified = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-1/positions")
                .header("x-auth-token", &auth_token)
                .header(header::IF_MODIFIED_SINCE, &last_modified)
                .body(Body::empty())
                .expect("book positions conditional request should build"),
        )
        .await
        .expect("book positions conditional request should complete");

    assert_eq!(not_modified.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(
        not_modified
            .headers()
            .get(header::LAST_MODIFIED)
            .and_then(|value| value.to_str().ok()),
        Some(last_modified.as_str())
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_book_pages_persisted_pdf_rows_match_kotlin_dynamic_page_shape() {
    let paths = new_router_fixture("router-book-pages-persisted-pdf-dynamic-shape").await;
    seed_router_contract_data(&paths).await;
    seed_router_pdf_book(
        &paths,
        "book-pdf-1",
        "series-1",
        "fixture-page.pdf",
        "Fixture PDF",
    )
    .await;
    seed_router_persisted_pdf_page(&paths, "book-pdf-1", 1, "page-1.pdf", 612, 866, None).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/books/book-pdf-1/pages")
                .header("x-auth-token", &auth_token)
                .body(Body::empty())
                .expect("persisted pdf pages request should build"),
        )
        .await
        .expect("persisted pdf pages request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    let payload = response_json(response).await;
    let rows = payload
        .as_array()
        .expect("persisted pdf pages payload should be an array");
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].get("fileName"),
        Some(&Value::String("page-1.pdf".to_string()))
    );
    assert_eq!(
        rows[0].get("mediaType"),
        Some(&Value::String("image/jpeg".to_string()))
    );
    assert_eq!(rows[0].get("width"), Some(&json!(3200)));
    assert_eq!(rows[0].get("height"), Some(&json!(4528)));
    assert!(rows[0].get("sizeBytes").is_some_and(Value::is_null));
    assert_eq!(rows[0].get("size"), Some(&Value::String(String::new())));

    cleanup_router_fixture(paths);
}

async fn update_book_search_fixture_title(paths: &RuntimeDbPaths, book_id: &str, title: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("books search parity db should open for title update");

    sqlx::query(
        "UPDATE BOOK_METADATA \
         SET TITLE = ? \
         WHERE BOOK_ID = ?",
    )
    .bind(title)
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("books search parity title should update");

    pool.close().await;
}

async fn seed_router_persisted_pdf_page(
    paths: &RuntimeDbPaths,
    book_id: &str,
    number: i64,
    file_name: &str,
    width: i64,
    height: i64,
    file_size: Option<i64>,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("persisted pdf page db should open");

    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT, FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(number)
    .bind("")
    .bind(file_name)
    .bind("application/pdf")
    .bind(width)
    .bind(height)
    .bind(file_size)
    .execute(&pool)
    .await
    .expect("persisted pdf page row should be inserted");

    pool.close().await;
}

async fn books_list_ids(
    app: &axum::Router,
    auth_token: &str,
    sort: Option<&str>,
    full_text_search: Option<&str>,
) -> Vec<String> {
    let mut uri = String::from("/api/v1/books/list?page=0&size=20");
    if let Some(sort) = sort {
        uri.push_str("&sort=");
        uri.push_str(sort);
    }

    let mut payload = json!({
        "condition": {
            "type": "Title",
            "operator": "contains",
            "value": "book"
        }
    });
    if let Some(search) = full_text_search {
        payload["fullTextSearch"] = Value::String(search.to_string());
    }

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("x-auth-token", auth_token)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(payload.to_string()))
                .expect("books search parity request should build"),
        )
        .await
        .expect("books search parity request should complete");
    assert_eq!(response.status(), StatusCode::OK);

    let payload = response_json(response).await;
    payload
        .get("content")
        .and_then(Value::as_array)
        .expect("books search parity payload should expose content array")
        .iter()
        .filter_map(|entry| entry.get("id").and_then(Value::as_str).map(str::to_string))
        .collect()
}

#[tokio::test]
async fn router_discovery_books_list_locks_main_search_parity_for_retained_inputs() {
    let paths = new_router_fixture("router-discovery-books-list-main-search-parity").await;
    seed_router_contract_data(&paths).await;
    seed_router_authors_scope_variants(&paths).await;
    update_book_search_fixture_title(&paths, "book-2", "Book Book 2").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let blank_ids = books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("   ")).await;
    assert_eq!(blank_ids, vec!["book-1", "book-2", "book-3"]);

    let relevance_desc_ids =
        books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("book")).await;
    assert_eq!(relevance_desc_ids, vec!["book-2", "book-1", "book-3"]);

    let relevance_asc_ids =
        books_list_ids(&app, &admin_token, Some("relevance,asc"), Some("book")).await;
    assert_eq!(relevance_asc_ids, vec!["book-3", "book-1", "book-2"]);

    let fielded_ids = books_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("title:book"),
    )
    .await;
    assert_eq!(fielded_ids, vec!["book-2", "book-1", "book-3"]);

    let invalid_query_ids =
        books_list_ids(&app, &admin_token, Some("relevance,desc"), Some("title:(")).await;
    assert!(invalid_query_ids.is_empty());

    seed_router_age_exclude_user_with_roles(
        &paths,
        "restricted-user",
        "restricted@example.org",
        "router-contract-restricted-123",
        16,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
    let restricted_token = login_with_basic_credentials_and_get_token(
        app.clone(),
        "restricted@example.org",
        "router-contract-restricted-123",
    )
    .await;
    let visible_ids = books_list_ids(
        &app,
        &restricted_token,
        Some("relevance,desc"),
        Some("book"),
    )
    .await;
    assert_eq!(visible_ids, vec!["book-3"]);

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_retains_accent_folded_and_cjk_recall() {
    let paths = new_router_fixture("router-discovery-books-list-accent-cjk-recall").await;
    seed_router_contract_data(&paths).await;
    update_book_search_fixture_title(&paths, "book-1", "Café 東京 Book 1").await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let admin_token = login_with_basic_and_get_token(app.clone()).await;

    let accent_cjk_ids = books_list_ids(
        &app,
        &admin_token,
        Some("relevance,desc"),
        Some("cafe 東京"),
    )
    .await;
    assert_eq!(
        accent_cjk_ids,
        vec!["book-1"],
        "books/list should retain accent-folded mixed CJK recall at the route boundary",
    );

    cleanup_router_fixture(paths);
}

#[tokio::test]
async fn router_discovery_books_list_rejects_legacy_regex_search_body_input() {
    let paths = new_router_fixture("router-discovery-books-list-legacy-regex-search").await;
    seed_router_contract_data(&paths).await;

    let app = build_router_with_config(&runtime_config_for_paths(&paths));
    let auth_token = login_with_basic_and_get_token(app.clone()).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/books/list?page=0&size=20")
                .header("x-auth-token", &auth_token)
                .header("x-komga-runtime-search-ownership", "runtime-rust-owned")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({
                        "condition": {
                            "type": "Title",
                            "operator": "contains",
                            "value": "book"
                        },
                        "regexSearch": "book"
                    })
                    .to_string(),
                ))
                .expect("legacy books/list regexSearch request should build"),
        )
        .await
        .expect("legacy books/list regexSearch request should complete");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    cleanup_router_fixture(paths);
}
