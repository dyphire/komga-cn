use super::*;
use crate::sqlite::connect_test_pool;

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
    let pool = connect_test_pool(&db_path, 1)
        .await
        .expect("test db should open");
    crate::sqlite::setup::bootstrap_pool(&pool)
        .await
        .expect("test db should bootstrap main schema");

    let library_root = root.join("library-root");
    fs::create_dir_all(&library_root).expect("library root should be created");
    sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
        .bind("library-1")
        .bind("Library 1")
        .bind(library_root.to_string_lossy().to_string())
        .execute(&pool)
        .await
        .expect("library row should be inserted");
    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)",
    )
        .bind("series-1")
        .bind(0_i64)
        .bind("Series 1")
        .bind("series-one")
        .bind("library-1")
        .bind(0)
        .execute(&pool)
        .await
        .expect("series row should be inserted");

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

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series-two")
    .bind("library-1")
    .bind(0)
    .execute(&pool)
    .await
    .expect("second series row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, SERIES_ID, LIBRARY_ID, NAME, URL) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)",
    )
        .bind("book-upgrade")
        .bind(0_i64)
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
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, SERIES_ID, LIBRARY_ID, NAME, URL) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)",
    )
        .bind("book-upgrade")
        .bind(0_i64)
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

    result
        .expect("oneshot import should succeed")
        .expect("oneshot import should return an outcome");

    let expected_file = oneshot_dir.join("renamed.cbz");
    let expected_metadata_sidecar = oneshot_dir.join("renamed.xml");
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
        expected_numbered_artwork_sidecar.exists(),
        "numbered artwork sidecars should preserve their numeric suffix on import"
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

#[tokio::test]
async fn import_book_upgrade_preserves_epub_extension_blob() {
    let (db_path, pool, root) = create_test_db("upgrade-preserves-epub-extension").await;
    let port = FilesystemImportPort::new(&db_path);
    let source_path = root.join("incoming.epub");
    fs::write(&source_path, b"epub-fixture").expect("source fixture should be written");

    let existing_dir = root.join("library-root/series-one");
    fs::create_dir_all(&existing_dir).expect("existing series directory should be created");
    fs::write(existing_dir.join("existing.epub"), b"existing-epub-fixture")
        .expect("existing upgraded file should exist");

    sqlx::query(
        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, SERIES_ID, LIBRARY_ID, NAME, URL,
                          FILE_SIZE, NUMBER)
         VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("book-upgrade")
    .bind(0_i64)
    .bind("series-1")
    .bind("library-1")
    .bind("existing.epub")
    .bind("series-one/existing.epub")
    .bind(128_i64)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("upgrade book row should be inserted");
    sqlx::query(
        "INSERT INTO MEDIA (BOOK_ID, STATUS, EXTENSION_CLASS, EXTENSION_VALUE_BLOB) VALUES (?, ?, ?, ?)",
    )
    .bind("book-upgrade")
    .bind("READY")
    .bind("org.gotson.komga.domain.model.MediaExtensionEpub")
    .bind(vec![1_u8, 2, 3, 4, 5])
    .execute(&pool)
    .await
    .expect("source epub extension blob should be inserted");

    let result = port
        .import_book(
            ImportCopyMode::Copy,
            BooksImportEntry {
                source_file: source_path,
                series_id: "series-1".to_string(),
                destination_name: Some("restored".to_string()),
                upgrade_book_id: Some("book-upgrade".to_string()),
            },
        )
        .await
        .expect("upgrade import should succeed")
        .expect("upgrade import should return an outcome");
    let expected_file = root.join("library-root/series-one/restored.epub");
    let imported_book_id = result.imported_book_id;

    let imported_book = sqlx::query("SELECT URL FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(&imported_book_id)
        .fetch_one(&pool)
        .await
        .expect("migrated book row should be queryable");
    assert_eq!(
        imported_book.get::<String, _>("URL"),
        "series-one/restored.epub",
        "upgrade migration should persist library-relative book urls for imported files",
    );
    assert!(
        expected_file.exists(),
        "upgrade import should materialize the imported EPUB at the destination path",
    );

    let migrated_media = sqlx::query(
        "SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
    )
    .bind(&imported_book_id)
    .fetch_one(&pool)
    .await
    .expect("migrated media row should be queryable");
    assert_eq!(
        migrated_media
            .get::<Option<String>, _>("EXTENSION_CLASS")
            .as_deref(),
        Some("org.gotson.komga.domain.model.MediaExtensionEpub"),
        "upgrade migration should preserve the EPUB extension class when book identity changes",
    );
    assert_eq!(
        migrated_media.get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB"),
        Some(vec![1_u8, 2, 3, 4, 5]),
        "upgrade migration should preserve the EPUB extension blob when book identity changes",
    );

    pool.close().await;
}
