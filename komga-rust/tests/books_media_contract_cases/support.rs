use super::*;

pub(super) fn write_router_epub_with_cover(paths: &RuntimeDbPaths, relative_book_path: &str) {
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

pub(super) fn fixture_epub_positions_extension_blob() -> Vec<u8> {
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

pub(super) fn fixture_epub_positions_extension_blob_total_progression_021() -> Vec<u8> {
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

pub(super) fn fixture_epub_positions_extension_blob_total_progression_0995() -> Vec<u8> {
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

pub(super) fn fixture_epub_positions_extension_blob_without_total_progression() -> Vec<u8> {
    vec![
        31, 139, 8, 0, 130, 100, 226, 105, 2, 255, 37, 204, 61, 14, 128, 32, 20, 3, 224, 187,
        212, 209, 255, 193, 133, 171, 24, 7, 52, 40, 68, 244, 189, 0, 131, 134, 112, 119, 81,
        199, 246, 75, 27, 193, 228, 77, 48, 116, 122, 136, 49, 66, 59, 181, 66, 160, 157, 137,
        246, 186, 111, 46, 29, 14, 91, 172, 78, 110, 168, 16, 110, 86, 217, 36, 179, 53, 139,
        124, 55, 237, 231, 229, 117, 216, 204, 150, 254, 50, 31, 69, 176, 163, 205, 41, 239,
        115, 134, 232, 154, 33, 165, 41, 61, 85, 24, 32, 25, 108, 0, 0, 0,
    ]
}

pub(super) fn fixture_epub_positions_extension_blob_fixed_layout_single_position() -> Vec<u8> {
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

pub(super) async fn update_book_search_fixture_title(
    paths: &RuntimeDbPaths,
    book_id: &str,
    title: &str,
) {
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

pub(super) async fn seed_router_persisted_pdf_page(
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

pub(super) async fn books_list_ids(
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
