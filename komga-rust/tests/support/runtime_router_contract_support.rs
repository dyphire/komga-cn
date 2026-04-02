use std::collections::BTreeMap;
use std::fs::File;
use std::io::Write;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bcrypt::{DEFAULT_COST, hash as hash_bcrypt_password};
use komga_rust::config::{RuntimeCli, RuntimeConfig};
use komga_rust::infrastructure::sqlite::connect_pool;
use lopdf::{Document as PdfDocument, Object, Stream, dictionary};
use serde_json::Value;
use tower::util::ServiceExt;
use zip::CompressionMethod;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

#[path = "persistence_contract_fixture.rs"]
mod persistence_contract_fixture;

pub use persistence_contract_fixture::RuntimeDbPaths;

pub fn cleanup_router_fixture(paths: RuntimeDbPaths) {
    persistence_contract_fixture::cleanup(paths)
}

pub async fn new_router_fixture(case_id: &str) -> persistence_contract_fixture::RuntimeDbPaths {
    let paths = persistence_contract_fixture::new_runtime_db_paths(case_id)
        .expect("router contract fixture paths should be created");
    persistence_contract_fixture::seed_main_db_from_flyway(&paths.main_db)
        .await
        .expect("main db flyway fixture should be created");
    persistence_contract_fixture::seed_tasks_db_from_flyway(&paths.tasks_db)
        .await
        .expect("tasks db flyway fixture should be created");
    paths
}

pub fn runtime_config_for_paths(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
) -> RuntimeConfig {
    let mut env = BTreeMap::new();
    env.insert(
        "KOMGA_CONFIG_DIR".to_string(),
        paths.config_dir.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_DATABASE_FILE".to_string(),
        paths.main_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_TASKS_DB_FILE".to_string(),
        paths.tasks_db.to_string_lossy().to_string(),
    );
    env.insert(
        "KOMGA_RUST_RUNTIME_PROFILE".to_string(),
        "snapshot-aligned".to_string(),
    );

    RuntimeConfig::resolve_with_env(&RuntimeCli::default(), &env)
        .expect("runtime config should resolve fixture paths")
}

pub async fn seed_router_contract_data(paths: &persistence_contract_fixture::RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open");

    for sql in [
        "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_ID varchar NOT NULL DEFAULT ''",
        "ALTER TABLE READ_PROGRESS ADD COLUMN DEVICE_NAME varchar NOT NULL DEFAULT ''",
        "ALTER TABLE READ_PROGRESS ADD COLUMN LOCATOR blob",
    ] {
        let _ = sqlx::query(sql).execute(&pool).await;
    }

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
                 VALUES (?, ?, ?)",
    )
    .bind("library-1")
    .bind("Library 1")
    .bind(paths.config_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-1")
    .bind(0_i64)
    .bind("Series 1")
    .bind("series/series-1")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 1")
    .bind("Series 1")
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("SciFi")
    .execute(&pool)
    .await
    .expect("series metadata genre row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Favorite")
    .execute(&pool)
    .await
    .expect("series metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL) \
                 VALUES (?, ?)",
    )
    .bind("series-1")
    .bind("Family")
    .execute(&pool)
    .await
    .expect("series metadata sharing row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_AUTHOR (SERIES_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("series-1")
    .bind("John Doe")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata aggregation author row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("collection-1")
    .bind("Collection 1")
    .bind(false)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("collection row should be inserted");

    sqlx::query(
        "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) \
         VALUES (?, ?, ?)",
    )
    .bind("collection-1")
    .bind("series-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("collection series row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind(0_i64)
    .bind("book-1.epub")
    .bind("books/book-1.epub")
    .bind("series-1")
    .bind(1_024_i64)
    .bind(1_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("book row should be inserted");

    sqlx::query(
        "UPDATE BOOK \
                 SET FILE_HASH_KOREADER = ? \
                 WHERE ID = ?",
    )
    .bind("hash-book-1")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book koreader hash should be set");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-1")
    .bind(10_i64)
    .execute(&pool)
    .await
    .expect("media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("1")
    .bind(1.0_f64)
    .bind("Book 1")
    .bind("2024-01-15")
    .bind("book-1")
    .execute(&pool)
    .await
    .expect("book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) \
                 VALUES (?, ?)",
    )
    .bind("book-1")
    .bind("favorite-tag")
    .execute(&pool)
    .await
    .expect("book metadata tag row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
                 VALUES (?, ?, ?)",
    )
    .bind("book-1")
    .bind("Jane Writer")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("book metadata author row should be inserted");

    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("thumb-book-1")
    .bind("book-1")
    .bind("USER_UPLOADED")
    .bind(true)
    .execute(&pool)
    .await
    .expect("book thumbnail row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION (RELEASE_DATE, SUMMARY, SUMMARY_NUMBER, SERIES_ID) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("2024-01-15")
    .bind("")
    .bind("")
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("book metadata aggregation row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("ReadList 1")
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("readlist row should be inserted");

    sqlx::query(
        "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) \
                 VALUES (?, ?, ?)",
    )
    .bind("readlist-1")
    .bind("book-1")
    .bind(0_i64)
    .execute(&pool)
    .await
    .expect("readlist book row should be inserted");

    let hashed_password = hash_bcrypt_password("router-contract-admin-123", DEFAULT_COST)
        .expect("bcrypt hash should be computed");
    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("admin-user")
    .bind("admin@example.org")
    .bind(hashed_password)
    .bind(true)
    .execute(&pool)
    .await
    .expect("admin user should be inserted");

    for role in ["USER", "ADMIN", "FILE_DOWNLOAD", "PAGE_STREAMING"] {
        sqlx::query(
            "INSERT INTO USER_ROLE (USER_ID, ROLE) \
                     VALUES (?, ?)",
        )
        .bind("admin-user")
        .bind(role)
        .execute(&pool)
        .await
        .expect("admin role should be inserted");
    }

    pool.close().await;
}

#[allow(dead_code)]
pub async fn update_router_book_name(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    book_id: &str,
    name: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for book rename");

    sqlx::query("UPDATE BOOK SET NAME = ? WHERE ID = ?")
        .bind(name)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book name should be updated");

    pool.close().await;
}

#[allow(dead_code)]
pub fn write_router_epub_resource(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    relative_book_path: &str,
    resource_name: &str,
    resource_bytes: &[u8],
) {
    let epub_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = epub_path.parent() {
        std::fs::create_dir_all(parent).expect("epub parent directory should be created");
    }

    let file = File::create(&epub_path).expect("epub fixture file should be created");
    let mut zip = ZipWriter::new(file);
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .unix_permissions(0o644);

    zip.start_file("mimetype", options)
        .expect("mimetype entry should be created");
    zip.write_all(b"application/epub+zip")
        .expect("mimetype payload should be written");

    zip.start_file("META-INF/container.xml", options)
        .expect("container entry should be created");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
    )
    .expect("container payload should be written");

    zip.start_file("OEBPS/content.opf", options)
        .expect("package entry should be created");
    let media_type = if resource_name.ends_with(".html") || resource_name.ends_with(".xhtml") {
        "application/xhtml+xml"
    } else if resource_name.ends_with(".css") {
        "text/css"
    } else if resource_name.ends_with(".svg") {
        "image/svg+xml"
    } else if resource_name.ends_with(".png") {
        "image/png"
    } else if resource_name.ends_with(".jpg") || resource_name.ends_with(".jpeg") {
        "image/jpeg"
    } else {
        "application/octet-stream"
    };
    let package = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><package version=\"3.0\" xmlns=\"http://www.idpf.org/2007/opf\" unique-identifier=\"bookid\"><metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:identifier id=\"bookid\">book-1</dc:identifier><dc:title>Fixture Book</dc:title><dc:language>en</dc:language></metadata><manifest><item id=\"main\" href=\"{}\" media-type=\"{}\"/></manifest><spine><itemref idref=\"main\"/></spine></package>",
        resource_name, media_type,
    );
    zip.write_all(package.as_bytes())
        .expect("package payload should be written");

    zip.start_file(resource_name, options)
        .expect("resource entry should be created");
    zip.write_all(resource_bytes)
        .expect("resource payload should be written");

    zip.finish()
        .expect("epub fixture should finish successfully");
}

#[allow(dead_code)]
pub fn write_single_page_pdf_fixture(path: &std::path::Path) {
    let mut document = PdfDocument::with_version("1.5");
    let pages_id = document.new_object_id();
    let page_id = document.new_object_id();
    let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
    let resources_id = document.add_object(dictionary! {});

    document.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        }),
    );
    document.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );

    let catalog_id = document.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    document.trailer.set("Root", catalog_id);
    document.compress();
    document
        .save(path)
        .expect("single-page pdf fixture should be saved");
}

#[allow(dead_code)]
pub async fn seed_router_pdf_book(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    book_id: &str,
    series_id: &str,
    file_name: &str,
    title: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract db should open for pdf book seed");

    let relative_path = format!("books/{file_name}");
    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(&relative_path)
    .bind(series_id)
    .bind(4_096_i64)
    .bind(99_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("pdf book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("application/pdf")
    .bind("READY")
    .bind(book_id)
    .bind(1_i64)
    .execute(&pool)
    .await
    .expect("pdf media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) VALUES (?, ?, ?, ?, ?)",
    )
    .bind("99")
    .bind(99.0_f64)
    .bind(title)
    .bind("2024-02-01")
    .bind(book_id)
    .execute(&pool)
    .await
    .expect("pdf book metadata row should be inserted");

    pool.close().await;

    let pdf_path = paths.config_dir.join(relative_path);
    if let Some(parent) = pdf_path.parent() {
        std::fs::create_dir_all(parent).expect("pdf parent directory should be created");
    }
    write_single_page_pdf_fixture(&pdf_path);
}

#[allow(dead_code)]
pub fn fixture_png_bytes() -> Vec<u8> {
    vec![
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8,
        0xCF, 0xC0, 0xF0, 0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ]
}

#[allow(dead_code)]
pub fn multipart_image_upload_body(
    field_name: &str,
    file_name: &str,
    media_type: &str,
    selected: bool,
    bytes: &[u8],
) -> (String, Vec<u8>) {
    let boundary = "komga-rust-thumbnail-boundary";
    let mut body = Vec::new();
    write!(
        &mut body,
        "--{boundary}\r\nContent-Disposition: form-data; name=\"{field_name}\"; filename=\"{file_name}\"\r\nContent-Type: {media_type}\r\n\r\n"
    )
    .expect("multipart file prelude should be written");
    body.extend_from_slice(bytes);
    write!(
        &mut body,
        "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"selected\"\r\n\r\n{}\r\n--{boundary}--\r\n",
        if selected { "true" } else { "false" }
    )
    .expect("multipart selected field should be written");

    (format!("multipart/form-data; boundary={boundary}"), body)
}

#[allow(dead_code)]
pub async fn seed_router_contract_nullable_samples(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract nullable db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("NullPub")
    .bind("EN")
    .bind(18_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series metadata row should be inserted");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(1_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("nullable series book count should be updated");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, \
           LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("nullable book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA (MEDIA_TYPE, STATUS, BOOK_ID, PAGE_COUNT) \
                 VALUES (?, ?, ?, ?)",
    )
    .bind("application/epub+zip")
    .bind("READY")
    .bind("book-2")
    .bind(12_i64)
    .execute(&pool)
    .await
    .expect("nullable media row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("nullable book metadata row should be inserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_read_progress(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    completed: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED) \
         VALUES (?, ?, ?, ?)",
    )
    .bind("book-1")
    .bind("admin-user")
    .bind(if completed { 10_i64 } else { 1_i64 })
    .bind(completed)
    .execute(&pool)
    .await
    .expect("router contract read-progress row should be inserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_read_progress(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    read_count: i64,
    in_progress_count: i64,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series read-progress db should open");

    sqlx::query(
        "INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT) \
         VALUES (?, ?, ?, ?) \
         ON CONFLICT (SERIES_ID, USER_ID) DO UPDATE \
         SET READ_COUNT = excluded.READ_COUNT, IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT",
    )
    .bind("series-1")
    .bind("admin-user")
    .bind(read_count)
    .bind(in_progress_count)
    .execute(&pool)
    .await
    .expect("router contract series read-progress row should be upserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_counts(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    book_count: i64,
    total_book_count: Option<i64>,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series counts db should open");

    sqlx::query(
        "UPDATE SERIES \
                 SET BOOK_COUNT = ? \
                 WHERE ID = ?",
    )
    .bind(book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series book_count should be updated");

    sqlx::query(
        "UPDATE SERIES_METADATA \
                 SET TOTAL_BOOK_COUNT = ? \
                 WHERE SERIES_ID = ?",
    )
    .bind(total_book_count)
    .bind("series-1")
    .execute(&pool)
    .await
    .expect("router contract series total_book_count should be updated");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_age_exclude_user(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    age_restriction: i64,
) {
    seed_router_age_exclude_user_with_roles(
        paths,
        user_id,
        email,
        password,
        age_restriction,
        &["USER", "PAGE_STREAMING"],
    )
    .await;
}

#[allow(dead_code)]
pub async fn seed_router_age_exclude_user_with_roles(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    age_restriction: i64,
    roles: &[&str],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract restricted-user db should open");

    let hashed_password =
        hash_bcrypt_password(password, DEFAULT_COST).expect("bcrypt hash should be computed");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION, AGE_RESTRICTION_ALLOW_ONLY) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(true)
    .bind(age_restriction)
    .bind(false)
    .execute(&pool)
    .await
    .expect("restricted user should be inserted");

    for role in roles {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(*role)
            .execute(&pool)
            .await
            .expect("restricted role should be inserted");
    }

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_library_restricted_user(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    user_id: &str,
    email: &str,
    password: &str,
    library_ids: &[&str],
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract library-restricted-user db should open");

    let hashed_password = hash_bcrypt_password(password, DEFAULT_COST)
        .expect("restricted user password hash should be computed");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, ?, ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(hashed_password)
    .bind(false)
    .execute(&pool)
    .await
    .expect("library-restricted user should be inserted");

    for role in ["USER", "PAGE_STREAMING"] {
        sqlx::query("INSERT INTO USER_ROLE (USER_ID, ROLE) VALUES (?, ?)")
            .bind(user_id)
            .bind(role)
            .execute(&pool)
            .await
            .expect("library-restricted role should be inserted");
    }

    for library_id in library_ids {
        sqlx::query("INSERT INTO USER_LIBRARY_SHARING (USER_ID, LIBRARY_ID) VALUES (?, ?)")
            .bind(user_id)
            .bind(library_id)
            .execute(&pool)
            .await
            .expect("library sharing row should be inserted");
    }

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_title_sort(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    title_sort: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series title-sort db should open");

    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE_SORT = ? \
         WHERE SERIES_ID = ?",
    )
    .bind(title_sort)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("series metadata title_sort should be updated for contract test");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_alternate_title(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    label: &str,
    title: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series alternate-title db should open");

    sqlx::query(
        "INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE) \
         VALUES (?, ?, ?)",
    )
    .bind(series_id)
    .bind(label)
    .bind(title)
    .execute(&pool)
    .await
    .expect("series metadata alternate title should be inserted for contract test");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_series_aggregated_tag(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    tag: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract series aggregated tag db should open");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AGGREGATION_TAG (SERIES_ID, TAG) \
         VALUES (?, ?)",
    )
    .bind(series_id)
    .bind(tag)
    .execute(&pool)
    .await
    .expect("series aggregated tag row should be inserted for contract test");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_custom_series(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
    series_id: &str,
    name: &str,
    library_id: &str,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract custom series db should open");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(series_id)
    .bind(0_i64)
    .bind(name)
    .bind(format!("series/{series_id}"))
    .bind(library_id)
    .execute(&pool)
    .await
    .expect("custom series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, \
           SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind(name)
    .bind(name)
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind(series_id)
    .execute(&pool)
    .await
    .expect("custom series metadata row should be inserted");

    pool.close().await;
}

#[allow(dead_code)]
pub async fn seed_router_authors_scope_variants(
    paths: &persistence_contract_fixture::RuntimeDbPaths,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("router contract authors scope db should open");

    sqlx::query(
        "INSERT INTO LIBRARY (ID, NAME, ROOT) \
         VALUES (?, ?, ?)",
    )
    .bind("library-2")
    .bind("Library 2")
    .bind(paths.config_dir.to_string_lossy().to_string())
    .execute(&pool)
    .await
    .expect("secondary library row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-2")
    .bind(0_i64)
    .bind("Series 2")
    .bind("series/series-2")
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary same-library series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 2")
    .bind("Series 2")
    .bind("PubHouse")
    .bind("EN")
    .bind(16_i64)
    .bind("series-2")
    .execute(&pool)
    .await
    .expect("secondary same-library series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-2")
    .bind(0_i64)
    .bind("book-2.epub")
    .bind("books/book-2.epub")
    .bind("series-2")
    .bind(2_048_i64)
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("secondary same-library book row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("2")
    .bind(2.0_f64)
    .bind("Book 2")
    .bind("2024-01-16")
    .bind("book-2")
    .execute(&pool)
    .await
    .expect("secondary same-library book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("book-2")
    .bind("Alex Side")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("secondary same-library book author row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("series-3")
    .bind(0_i64)
    .bind("Series 3")
    .bind("series/series-3")
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("cross-library series row should be inserted");

    sqlx::query(
        "INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, PUBLISHER, LANGUAGE, AGE_RATING, SERIES_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("ONGOING")
    .bind("Series 3")
    .bind("Series 3")
    .bind("AltPub")
    .bind("EN")
    .bind(12_i64)
    .bind("series-3")
    .execute(&pool)
    .await
    .expect("cross-library series metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("book-3")
    .bind(0_i64)
    .bind("book-3.epub")
    .bind("books/book-3.epub")
    .bind("series-3")
    .bind(4_096_i64)
    .bind(3_i64)
    .bind("library-2")
    .execute(&pool)
    .await
    .expect("cross-library book row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, RELEASE_DATE, BOOK_ID) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("3")
    .bind(3.0_f64)
    .bind("Book 3")
    .bind("2024-01-17")
    .bind("book-3")
    .execute(&pool)
    .await
    .expect("cross-library book metadata row should be inserted");

    sqlx::query(
        "INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) \
         VALUES (?, ?, ?)",
    )
    .bind("book-3")
    .bind("Morgan Else")
    .bind("writer")
    .execute(&pool)
    .await
    .expect("cross-library book author row should be inserted");

    pool.close().await;
}

pub async fn login_with_basic_and_get_token(app: axum::Router) -> String {
    login_with_basic_credentials_and_get_token(
        app,
        "admin@example.org",
        "router-contract-admin-123",
    )
    .await
}

pub async fn login_with_basic_credentials_and_get_token(
    app: axum::Router,
    email: &str,
    password: &str,
) -> String {
    let basic_token = STANDARD.encode(format!("{email}:{password}"));
    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_token}"))
                .body(Body::empty())
                .expect("users/me request should build"),
        )
        .await
        .expect("users/me request should complete");

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .and_then(|value| value.to_str().ok())
        .map(str::to_string)
        .expect("users/me login should return x-auth-token")
}

pub async fn response_json(response: axum::response::Response) -> Value {
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body should be readable");
    serde_json::from_slice(&body).expect("response body should be valid json")
}
