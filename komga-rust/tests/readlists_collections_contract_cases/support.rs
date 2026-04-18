use super::*;

pub(super) async fn seed_collection_listing_variants(paths: &RuntimeDbPaths) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("collection listing variants db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("secondary library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Search Target")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("secondary series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Search Target")
    .bind("Search Target")
    .bind("SecondPub")
    .bind("FR")
    .bind(12_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary series metadata row should be inserted");

    sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
        .bind("collection-2")
        .bind("Beta Collection")
        .bind(false)
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("secondary collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-2")
    .bind("series-2")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("secondary collection series row should be inserted");

    pool.close().await;
}

pub(super) async fn seed_collection_series_variants(paths: &RuntimeDbPaths) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("collection series variants db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("secondary library for collection series should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("secondary series for collection series should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ENDED")
    .bind("Series 2")
    .bind("Series 2")
    .bind("OtherPub")
    .bind("FR")
    .bind(18_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary series metadata for collection series should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-2")
        .bind("Drama")
        .execute(&pool)
        .await
        .expect("secondary series genre should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) VALUES (?, ?, ?)",
    )
    .bind("series-2")
    .bind("Alice Roe")
    .bind("editor")
    .execute(&pool)
    .await
    .expect("secondary series author should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-2")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("secondary series should be attached to collection-1");

    pool.close().await;
}

pub(super) async fn seed_readlist_endpoint_variants(paths: &RuntimeDbPaths) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist endpoint variants db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary readlist series should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("ONGOING")
        .bind("Series 2")
        .bind("Series 2")
        .bind("PubHouse")
        .bind("EN")
        .bind(16_i64)
        .bind("series-2")
        .execute(&pool)
        .await
        .expect("secondary readlist series metadata should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("2024-01-01")
    .bind("")
    .bind("")
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary readlist book metadata aggregation should be inserted");

    sqlx::query("INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("book-2")
        .bind(0_i64)
        .bind("books/book-2.epub")
        .bind("books/book-2.epub")
        .bind("series-2")
        .bind(2_048_i64)
        .bind(2_i64)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("secondary readlist book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(11_i64)
        .execute(&pool)
        .await
        .expect("secondary readlist media should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)")
        .bind("2")
        .bind(2.0_f64)
        .bind("Book 2")
        .bind("2024-01-16")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("secondary readlist book metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-2")
        .bind("Jane Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("secondary readlist book author should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-2")
        .bind("library-one-tag")
        .execute(&pool)
        .await
        .expect("secondary readlist book tag should be inserted");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("secondary readlist library should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-3")
    .bind(0_i64)
    .bind("Series 3")
    .bind("series/series-3")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("third readlist series should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("ONGOING")
        .bind("Series 3")
        .bind("Series 3")
        .bind("OtherPub")
        .bind("FR")
        .bind(12_i64)
        .bind("series-3")
        .execute(&pool)
        .await
        .expect("third readlist series metadata should be inserted");

    sqlx::query("INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind("book-3")
        .bind(0_i64)
        .bind("books/book-3.epub")
        .bind("books/book-3.epub")
        .bind("series-3")
        .bind(3_072_i64)
        .bind(3_i64)
        .bind("library-2")
        .execute(&pool)
        .await
        .expect("third readlist book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-3")
        .bind(12_i64)
        .execute(&pool)
        .await
        .expect("third readlist media should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)")
        .bind("3")
        .bind(3.0_f64)
        .bind("Book 3")
        .bind("2024-01-17")
        .bind("book-3")
        .execute(&pool)
        .await
        .expect("third readlist book metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Guest Writer")
        .bind("writer")
        .execute(&pool)
        .await
        .expect("third readlist book author should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-3")
        .bind("library-two-tag")
        .execute(&pool)
        .await
        .expect("third readlist book tag should be inserted");

    sqlx::query("UPDATE READLIST SET BOOK_COUNT = ? WHERE ID = ?")
        .bind(3_i64)
        .bind("readlist-1")
        .execute(&pool)
        .await
        .expect("readlist book count should be updated");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-2")
        .bind(1_i64)
        .execute(&pool)
        .await
        .expect("second readlist book relation should be inserted");

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind("readlist-1")
        .bind("book-3")
        .bind(2_i64)
        .execute(&pool)
        .await
        .expect("third readlist book relation should be inserted");

    pool.close().await;
}

pub(super) async fn mark_readlist_unordered(paths: &RuntimeDbPaths, readlist_id: &str) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unordered readlist db should open");

    sqlx::query("UPDATE READLIST SET ORDERED = ? WHERE ID = ?")
        .bind(false)
        .bind(readlist_id)
        .execute(&pool)
        .await
        .expect("readlist ordered flag should be updated");

    sqlx::query("UPDATE READLIST_BOOK SET NUMBER = ? WHERE READLIST_ID = ? AND BOOK_ID = ?")
        .bind(2_i64)
        .bind(readlist_id)
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("unordered readlist should move book-2 after book-3 in relation order");

    sqlx::query("UPDATE READLIST_BOOK SET NUMBER = ? WHERE READLIST_ID = ? AND BOOK_ID = ?")
        .bind(1_i64)
        .bind(readlist_id)
        .bind("book-3")
        .execute(&pool)
        .await
        .expect("unordered readlist should move book-3 before book-2 in relation order");

    pool.close().await;
}

pub(super) async fn seed_readlist_author_edge_case(paths: &RuntimeDbPaths) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("readlist author edge case db should open");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Doe, John")
        .bind("")
        .execute(&pool)
        .await
        .expect("edge-case readlist author should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
        .bind("book-3")
        .bind("Casey Role")
        .bind("CoWriter")
        .execute(&pool)
        .await
        .expect("mixed-case readlist author should be inserted");

    pool.close().await;
}

pub(super) async fn seed_facet_scope_variants(paths: &RuntimeDbPaths) {
    let pool = connect_test_pool(paths.main_db.as_path(), 1)
        .await
        .expect("facet scope variants db should open");

    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-2")
        .bind("Library 2")
        .bind(
            paths
                .config_dir
                .join("library-2")
                .to_string_lossy()
                .to_string(),
        )
        .execute(&pool)
        .await
        .expect("facet secondary library should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("facet secondary series should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("OtherPub")
    .bind("FR")
    .bind(12_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("facet secondary series metadata should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) VALUES (?, ?)")
        .bind("series-2")
        .bind("Drama")
        .execute(&pool)
        .await
        .expect("facet secondary genre should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) VALUES (?, ?)")
        .bind("series-2")
        .bind("other-series-tag")
        .execute(&pool)
        .await
        .expect("facet secondary series tag should be inserted");

    sqlx::query("INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) VALUES (?, ?)")
        .bind("series-2")
        .bind("Friends")
        .execute(&pool)
        .await
        .expect("facet secondary sharing label should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("facet secondary book should be inserted");

    sqlx::query("INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) VALUES (?, ?, ?, ?)")
        .bind("application/epub+zip")
        .bind("READY")
        .bind("book-2")
        .bind(11_i64)
        .execute(&pool)
        .await
        .expect("facet secondary media should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)")
        .bind("2")
        .bind(2.0_f64)
        .bind("Book 2")
        .bind("2025-02-20")
        .bind("book-2")
        .execute(&pool)
        .await
        .expect("facet secondary book metadata should be inserted");

    sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
        .bind("book-2")
        .bind("other-book-tag")
        .execute(&pool)
        .await
        .expect("facet secondary book tag should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) VALUES (?, ?, ?, ?)",
    )
    .bind("2025-02-20")
    .bind("")
    .bind("")
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("facet secondary book metadata aggregation should be inserted");

    pool.close().await;
}

pub(super) fn comicrack_multipart_body(xml: &str) -> (String, Vec<u8>) {
    let boundary = "komga-rust-comicrack-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"list.cbl\"\r\nContent-Type: application/xml\r\n\r\n{xml}\r\n--{boundary}--\r\n"
    );

    (
        format!("multipart/form-data; boundary={boundary}"),
        body.into_bytes(),
    )
}

pub(super) fn comicrack_multipart_body_with_quoted_boundary(xml: &str) -> (String, Vec<u8>) {
    let boundary = "komga-rust-quoted-boundary";
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"list.cbl\"\r\nContent-Type: application/xml\r\n\r\n{xml}\r\n--{boundary}--\r\n"
    );

    (
        format!("multipart/form-data; boundary=\"{boundary}\""),
        body.into_bytes(),
    )
}
