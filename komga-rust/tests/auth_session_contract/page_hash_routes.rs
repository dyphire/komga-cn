use super::*;

fn large_png_bytes(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbaImage::from_fn(width.max(1), height.max(1), |x, y| {
        image::Rgba([(x % 255) as u8, (y % 255) as u8, ((x + y) % 255) as u8, 255])
    });
    let mut output = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image)
        .write_to(&mut output, image::ImageFormat::Png)
        .expect("png fixture should encode");
    output.into_inner()
}

async fn seed_unknown_page_hash_source(
    paths: &RuntimeDbPaths,
    book_id: &str,
    hash: &str,
    relative_book_path: &str,
    file_name: &str,
    media_type: &str,
    bytes: &[u8],
) -> std::path::PathBuf {
    let source_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = source_path.parent() {
        std::fs::create_dir_all(parent).expect("unknown page hash source parent should be created");
    }
    std::fs::write(&source_path, bytes).expect("unknown page hash source should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unknown page hash source db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(relative_book_path)
    .bind("series-1")
    .bind(i64::try_from(bytes.len()).expect("source bytes length should fit i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("unknown page hash source book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(hash)
    .bind(file_name)
    .bind(media_type)
    .bind(i64::try_from(bytes.len()).expect("source bytes length should fit i64"))
    .execute(&pool)
    .await
    .expect("unknown page hash source media page row should be inserted");

    pool.close().await;
    source_path
}

async fn seed_unknown_page_hash_pdf_match(paths: &RuntimeDbPaths, book_id: &str, hash: &str) {
    seed_router_pdf_book(
        paths,
        book_id,
        "series-1",
        "unknown-page-hash-source.pdf",
        "Unknown PDF Page",
    )
    .await;

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unknown page hash pdf db should open");
    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(hash)
    .bind("1")
    .bind("image/jpeg")
    .bind(4_096_i64)
    .execute(&pool)
    .await
    .expect("unknown page hash pdf media page row should be inserted");
    pool.close().await;
}

async fn seed_known_page_hash_samples(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("known page hash sample db should open");

    for (book_id, name, url, hash, page_number, file_size) in [
        (
            "book-known-1",
            "book-known-1.epub",
            "books/book-known-1.epub",
            "alpha-hash",
            10_i64,
            111_i64,
        ),
        (
            "book-known-2",
            "book-known-2.epub",
            "books/book-known-2.epub",
            "alpha-hash",
            11_i64,
            111_i64,
        ),
        (
            "book-known-3",
            "book-known-3.epub",
            "books/book-known-3.epub",
            "beta-hash",
            12_i64,
            222_i64,
        ),
        (
            "book-known-4",
            "book-known-4.epub",
            "books/book-known-4.epub",
            "gamma-hash",
            13_i64,
            333_i64,
        ),
        (
            "book-known-5",
            "book-known-5.epub",
            "books/book-known-5.epub",
            "gamma-hash",
            14_i64,
            333_i64,
        ),
        (
            "book-known-6",
            "book-known-6.epub",
            "books/book-known-6.epub",
            "gamma-hash",
            15_i64,
            333_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(page_number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("known page hash sample book row should be inserted");

        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(hash)
        .bind(format!("{book_id}.png"))
        .bind("image/png")
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("known page hash sample media page row should be inserted");
    }

    for (hash, size, action, delete_count, created_date, last_modified_date) in [
        (
            "alpha-hash",
            Some(120_i64),
            "IGNORE",
            1_i64,
            "2024-01-01 00:00:00",
            "2024-01-05 00:00:00",
        ),
        (
            "beta-hash",
            Some(220_i64),
            "DELETE_AUTO",
            2_i64,
            "2024-01-02 00:00:00",
            "2024-01-03 00:00:00",
        ),
        (
            "gamma-hash",
            Some(320_i64),
            "DELETE_MANUAL",
            0_i64,
            "2024-01-03 00:00:00",
            "2024-01-04 00:00:00",
        ),
    ] {
        sqlx::query(
            "INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(hash)
        .bind(size)
        .bind(action)
        .bind(delete_count)
        .bind(created_date)
        .bind(last_modified_date)
        .execute(&pool)
        .await
        .expect("known page hash row should be inserted");
    }

    pool.close().await;
}

fn delete_match_payload(
    book_id: &str,
    url: &str,
    page_number: i64,
    file_name: &str,
    file_size: i64,
    media_type: &str,
) -> String {
    json!({
        "bookId": book_id,
        "url": url,
        "pageNumber": page_number,
        "fileName": file_name,
        "fileSize": file_size,
        "mediaType": media_type,
    })
    .to_string()
}

#[path = "page_hash_routes/delete_tasks.rs"]
mod delete_tasks;
#[path = "page_hash_routes/list_and_match_queries.rs"]
mod list_and_match_queries;
#[path = "page_hash_routes/put_and_upsert.rs"]
mod put_and_upsert;
#[path = "page_hash_routes/unknown_thumbnails.rs"]
mod unknown_thumbnails;
