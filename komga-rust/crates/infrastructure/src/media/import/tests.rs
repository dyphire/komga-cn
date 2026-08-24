use super::*;
use crate::persistence::sqlite::connect_test_pool;
use std::time::{SystemTime, UNIX_EPOCH};

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

struct ImportTestFixture {
    pool: sqlx::Pool<sqlx::Sqlite>,
    root: PathBuf,
}

async fn create_import_fixture(case: &str) -> ImportTestFixture {
    let root = unique_temp_dir(case);
    fs::create_dir_all(&root).expect("temp root should be created");
    let db_path = root.join("import.sqlite");
    let pool = connect_test_pool(&db_path, 1)
        .await
        .expect("test db should open");
    crate::persistence::sqlite::schema::bootstrap_pool(&pool)
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

    ImportTestFixture { pool, root }
}

#[tokio::test]
async fn import_book_returns_error_when_source_file_is_missing() {
    let ImportTestFixture { pool, root } = create_import_fixture("missing-source").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());

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
        error.to_string().contains("source file does not exist"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[cfg(unix)]
#[test]
fn import_copy_mode_reports_source_metadata_errors() {
    let root = unique_temp_dir("copy-source-metadata-error");
    fs::create_dir_all(&root).expect("copy metadata fixture root should be created");
    fs::write(root.join("blocked"), b"not a directory")
        .expect("blocking source component should be written");

    let error = apply_import_copy_mode(
        ImportCopyMode::Copy,
        root.join("blocked/book.cbz").as_path(),
        root.join("destination.cbz").as_path(),
        false,
    )
    .expect_err("source metadata error should be propagated");

    assert!(
        error.to_string().contains("inspect import source file"),
        "unexpected import copy error: {error}"
    );

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn source_inside_library_roots_reports_library_root_probe_errors() {
    let root = unique_temp_dir("library-root-probe-error");
    fs::create_dir_all(&root).expect("library root probe fixture root should be created");
    let source_path = root.join("incoming.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");
    let blocked_parent = root.join("blocked");
    fs::write(&blocked_parent, b"not a directory")
        .expect("blocking library root component should be written");

    let error = source_inside_library_roots(&source_path, &[blocked_parent.join("library")])
        .expect_err("library root probe errors must not be treated as external source");

    assert!(
        error
            .to_string()
            .contains("canonicalize import library root"),
        "unexpected library root probe error: {error}"
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn import_book_returns_error_when_series_target_is_missing() {
    let ImportTestFixture { pool, root } = create_import_fixture("missing-series-target").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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
        error.to_string().contains("series target") || error.to_string().contains("missing-series"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_destination_name_is_invalid() {
    let ImportTestFixture { pool, root } = create_import_fixture("invalid-destination-name").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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
        error.to_string().contains("destination") || error.to_string().contains("nested/book.cbz"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_upgrade_target_series_mismatches() {
    let ImportTestFixture { pool, root } = create_import_fixture("upgrade-series-mismatch").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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
        error.to_string().contains("upgrade") || error.to_string().contains("series"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_upgrade_target_is_missing() {
    let ImportTestFixture { pool, root } = create_import_fixture("upgrade-target-missing").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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
        error.to_string().contains("upgrade") || error.to_string().contains("missing-upgrade-book"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_validates_library_roots_before_target_lookup() {
    let ImportTestFixture { pool, root } =
        create_import_fixture("source-inside-library-root").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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
        error.to_string().contains("existing library")
            || error.to_string().contains("part of an existing library"),
        "unexpected import error: {error}"
    );

    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("external source fixture should be written");

    sqlx::query("ALTER TABLE LIBRARY RENAME TO LIBRARY_BROKEN")
        .execute(&pool)
        .await
        .expect("library table should be renamed");

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

    let error = result.expect_err("library root query failure should return an error");
    assert!(error.to_string().contains("query library roots"), "{error}");

    pool.close().await;
}

#[tokio::test]
async fn import_book_propagates_historical_event_persistence_errors() {
    let ImportTestFixture { pool, root } =
        create_import_fixture("historical-event-persistence-error").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
    let source_path = root.join("book.cbz");
    fs::write(&source_path, b"fixture").expect("source fixture should be written");
    sqlx::query("DROP TABLE HISTORICAL_EVENT")
        .execute(&pool)
        .await
        .expect("historical event table should be dropped for persistence error fixture");

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

    let error = result.expect_err("historical event persistence errors should fail import");
    assert!(
        error.to_string().contains("historical") || error.to_string().contains("BookImported"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_returns_error_when_oneshot_series_missing_upgrade_book_id() {
    let ImportTestFixture { pool, root } =
        create_import_fixture("oneshot-missing-upgrade-book-id").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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
        error.to_string().contains("oneshot") || error.to_string().contains("upgradeBookId"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}

#[tokio::test]
async fn import_book_uses_oneshot_parent_directory_and_destination_basename() {
    let ImportTestFixture { pool, root } =
        create_import_fixture("oneshot-parent-directory-destination").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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

#[cfg(unix)]
#[test]
fn collect_book_sidecar_paths_reports_candidate_metadata_errors() {
    let root = unique_temp_dir("sidecar-metadata-error");
    fs::create_dir_all(&root).expect("sidecar metadata fixture root should be created");
    let book_path = root.join("incoming.cbz");
    fs::write(&book_path, b"fixture").expect("book fixture should be written");
    std::os::unix::fs::symlink(root.join("missing.xml"), root.join("incoming.xml"))
        .expect("broken sidecar symlink should be created");

    let error = collect_book_sidecar_paths(&book_path)
        .expect_err("sidecar metadata errors must not be treated as absent sidecars");

    assert!(
        error.to_string().contains("read book sidecar metadata"),
        "unexpected sidecar collection error: {error}"
    );

    let _ = fs::remove_dir_all(root);
}

#[tokio::test]
async fn import_book_upgrade_preserves_epub_extension_blob() {
    let ImportTestFixture { pool, root } =
        create_import_fixture("upgrade-preserves-epub-extension").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
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

#[tokio::test]
async fn import_book_upgrade_reports_old_file_removal_errors() {
    let ImportTestFixture { pool, root } =
        create_import_fixture("upgrade-old-file-removal-error").await;
    let port = FilesystemBookImport::new(pool.clone(), pool.clone());
    let source_path = root.join("incoming.cbz");
    fs::write(&source_path, b"new-fixture").expect("source fixture should be written");

    let existing_dir = root.join("library-root/series-one");
    fs::create_dir_all(&existing_dir).expect("existing series directory should be created");
    fs::create_dir(existing_dir.join("existing.cbz"))
        .expect("existing upgraded path should be a non-removable file replacement");

    sqlx::query(
        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, SERIES_ID, LIBRARY_ID, NAME, URL,
                          FILE_SIZE, NUMBER)
         VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?)"#,
    )
    .bind("book-upgrade")
    .bind(0_i64)
    .bind("series-1")
    .bind("library-1")
    .bind("existing.cbz")
    .bind("series-one/existing.cbz")
    .bind(128_i64)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("upgrade book row should be inserted");

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
        .await;

    let error = result.expect_err("upgrade cleanup errors should fail import");
    assert!(
        error
            .to_string()
            .contains("remove previous upgraded book file"),
        "unexpected import error: {error}"
    );

    pool.close().await;
}
