use super::*;

fn unique_temp_dir(case: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "komga-import-{case}-{nanos}-{}",
        std::process::id()
    ))
}

async fn create_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>, PathBuf) {
    let root = unique_temp_dir(case);
    fs::create_dir_all(&root).expect("temp root should be created");
    let db_path = root.join("import.sqlite");
    let pool = connect_pool(&db_path, 1)
        .await
        .expect("test db should open");

    sqlx::query(
        "CREATE TABLE IF NOT EXISTS LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL)",
    )
    .execute(&pool)
    .await
    .expect("library table should be created");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS SERIES (ID varchar NOT NULL PRIMARY KEY, LIBRARY_ID varchar NOT NULL, URL varchar NOT NULL, oneshot integer NOT NULL DEFAULT 0)",
    )
    .execute(&pool)
    .await
    .expect("series table should be created");
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS BOOK (ID varchar NOT NULL PRIMARY KEY, SERIES_ID varchar NOT NULL, LIBRARY_ID varchar NOT NULL, NAME varchar NOT NULL, URL varchar NOT NULL)",
    )
    .execute(&pool)
    .await
    .expect("book table should be created");

    let library_root = root.join("library-root");
    fs::create_dir_all(&library_root).expect("library root should be created");
    sqlx::query("INSERT INTO LIBRARY (ID, ROOT) VALUES (?, ?)")
        .bind("library-1")
        .bind(library_root.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("library row should be inserted");
    sqlx::query("INSERT INTO SERIES (ID, LIBRARY_ID, URL, oneshot) VALUES (?, ?, ?, ?)")
        .bind("series-1")
        .bind("library-1")
        .bind("series-one")
        .bind(0)
        .execute(&pool)
        .await
        .expect("series row should be inserted");
    sqlx::query("INSERT INTO SERIES (ID, LIBRARY_ID, URL, oneshot) VALUES (?, ?, ?, ?)")
        .bind("series-2")
        .bind("library-1")
        .bind("series-two")
        .bind(0)
        .execute(&pool)
        .await
        .expect("second series row should be inserted");

    (db_path, pool, root)
}

#[tokio::test]
async fn import_book_returns_error_when_source_file_is_missing() {
    let (db_path, pool, root) = create_test_db("missing-source").await;
    let port = FilesystemImportPort::new(&db_path);

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: root.join("missing.cbz"),
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            },
        )
        .await;

    let error = result.expect_err("missing source import should return an error");
    assert!(
        error.contains("source file does not exist"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_series_target_is_missing() {
    let (db_path, pool, root) = create_test_db("missing-series-target").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "missing-series".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            },
        )
        .await;

    let error = result.expect_err("missing series target import should return an error");
    assert!(
        error.contains("series target") || error.contains("missing-series"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_destination_name_is_invalid() {
    let (db_path, pool, root) = create_test_db("invalid-destination-name").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: Some("nested/book.cbz".to_string()),
                upgrade_book_id: None,
            },
        )
        .await;

    let error = result.expect_err("invalid destination name import should return an error");
    assert!(
        error.contains("destination") || error.contains("nested/book.cbz"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_upgrade_target_series_mismatches() {
    let (db_path, pool, root) = create_test_db("upgrade-series-mismatch").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");

    sqlx::query("INSERT INTO BOOK (ID, SERIES_ID, LIBRARY_ID, NAME, URL) VALUES (?, ?, ?, ?, ?)")
        .bind("book-upgrade")
        .bind("series-2")
        .bind("library-1")
        .bind("existing.cbz")
        .bind("series-two/existing.cbz")
        .execute(&pool)
        .await
        .expect("upgrade book row should be inserted");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: Some("book-upgrade".to_string()),
            },
        )
        .await;

    let error = result.expect_err("upgrade series mismatch should return an error");
    assert!(
        error.contains("upgrade") || error.contains("series"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_upgrade_target_is_missing() {
    let (db_path, pool, root) = create_test_db("upgrade-target-missing").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: Some("missing-upgrade-book".to_string()),
            },
        )
        .await;

    let error = result.expect_err("missing upgrade target should return an error");
    assert!(
        error.contains("upgrade") || error.contains("missing-upgrade-book"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_source_file_is_inside_library_root() {
    let (db_path, pool, root) = create_test_db("source-inside-library-root").await;
    let port = FilesystemImportPort::new(&db_path);
    let library_root = root.join("library-root");
    let source_path = library_root.join("incoming/book.cbz");
    fs::create_dir_all(source_path.parent().expect("source parent should exist"))
        .expect("source parent should be created");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            },
        )
        .await;

    let error = result.expect_err("library-contained import should return an error");
    assert!(
        error.contains("existing library") || error.contains("part of an existing library"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_oneshot_series_missing_upgrade_book_id() {
    let (db_path, pool, root) = create_test_db("oneshot-missing-upgrade-book-id").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");

    sqlx::query("UPDATE SERIES SET URL = ?, oneshot = 1 WHERE ID = ?")
        .bind("oneshots/existing.cbz")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("oneshot series row should be updated");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: None,
                upgrade_book_id: None,
            },
        )
        .await;

    let error = result.expect_err("oneshot import without upgrade book should return an error");
    assert!(
        error.contains("oneshot") || error.contains("upgradeBookId"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_uses_oneshot_parent_directory_and_destination_basename() {
    let (db_path, pool, root) = create_test_db("oneshot-parent-directory-destination").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("incoming.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");
    fs::write(source_path.with_extension("xml"), b"metadata-fixture")
        .expect("source metadata sidecar should be written");
    fs::write(root.join("incoming.png"), b"artwork-fixture")
        .expect("source artwork sidecar should be written");
    fs::write(root.join("incoming-1.jpg"), b"secondary-artwork-fixture")
        .expect("source numbered artwork sidecar should be written");

    sqlx::query("UPDATE SERIES SET URL = ?, oneshot = 1 WHERE ID = ?")
        .bind("oneshots/existing.cbz")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("oneshot series row should be updated");
    sqlx::query("INSERT INTO BOOK (ID, SERIES_ID, LIBRARY_ID, NAME, URL) VALUES (?, ?, ?, ?, ?)")
        .bind("book-upgrade")
        .bind("series-1")
        .bind("library-1")
        .bind("existing.cbz")
        .bind("oneshots/existing.cbz")
        .execute(&pool)
        .await
        .expect("upgrade book row should be inserted");

    let oneshot_dir = root.join("library-root/oneshots");
    fs::create_dir_all(&oneshot_dir).expect("oneshot directory should be created");
    let existing_file = oneshot_dir.join("existing.cbz");
    fs::write(&existing_file, b"old-fixture").expect("existing upgraded file should exist");
    fs::write(oneshot_dir.join("existing.xml"), b"old-sidecar")
        .expect("existing upgraded sidecar should exist");
    fs::write(oneshot_dir.join("existing.png"), b"old-artwork")
        .expect("existing upgraded artwork sidecar should exist");
    fs::write(oneshot_dir.join("existing-1.jpg"), b"old-secondary-artwork")
        .expect("existing numbered artwork sidecar should exist");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: Some("renamed".to_string()),
                upgrade_book_id: Some("book-upgrade".to_string()),
            },
        )
        .await;

    let outcome = result.expect("oneshot import should succeed");
    let outcome = outcome.expect("oneshot import should return an outcome");
    assert!(
        outcome.sidecar_imported,
        "metadata sidecar import should be reported for follow-up task scheduling"
    );
    assert!(
        outcome.artwork_sidecar_imported,
        "artwork sidecar import should be reported for follow-up task scheduling"
    );

    let expected_file = oneshot_dir.join("renamed.cbz");
    let expected_metadata_sidecar = oneshot_dir.join("renamed.xml");
    let expected_artwork_sidecar = oneshot_dir.join("renamed.png");
    let expected_numbered_artwork_sidecar = oneshot_dir.join("renamed-1.jpg");
    assert!(
        expected_file.exists(),
        "oneshot import should target parent directory with source extension: {}",
        expected_file.display()
    );
    assert!(
        expected_metadata_sidecar.exists(),
        "metadata sidecar should be renamed alongside imported book"
    );
    assert!(
        expected_artwork_sidecar.exists(),
        "artwork sidecar should be renamed alongside imported book"
    );
    assert!(
        expected_numbered_artwork_sidecar.exists(),
        "numbered artwork sidecars should preserve their numeric suffix on import"
    );
    assert!(
        !oneshot_dir.join("existing.cbz/renamed.cbz").exists(),
        "oneshot import must not treat existing book file path as a directory"
    );
    assert!(
        !existing_file.exists(),
        "upgrade import should remove the previous oneshot file when destination differs"
    );
    assert!(
        !oneshot_dir.join("existing.xml").exists(),
        "upgrade import should remove the previous metadata sidecar when destination differs"
    );
    assert!(
        !oneshot_dir.join("existing.png").exists(),
        "upgrade import should remove the previous artwork sidecar when destination differs"
    );
    assert!(
        !oneshot_dir.join("existing-1.jpg").exists(),
        "upgrade import should remove the previous numbered artwork sidecar when destination differs"
    );

    pool.close().await;
}
