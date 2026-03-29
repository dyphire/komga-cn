use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use zip::ZipArchive;

use crate::sqlite::read_models::{
    load_page_hash_matches_page as load_page_hash_matches_page_model,
    load_page_hash_thumbnail as load_page_hash_thumbnail_model,
    load_page_hashes_page as load_page_hashes_page_model,
    load_page_hashes_unknown_page as load_page_hashes_unknown_page_model,
    load_unknown_page_hash_source,
};
use crate::sqlite::write_models::{
    delete_all_page_hash_matches as delete_all_page_hash_matches_model,
    delete_page_hash_match as delete_page_hash_match_model,
    upsert_page_hash as upsert_page_hash_model,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashThumbnail {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

pub async fn load_page_hashes_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    load_page_hashes_page_model(database_file, page, size).await
}

pub async fn load_page_hashes_unknown_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    load_page_hashes_unknown_page_model(database_file, page, size).await
}

pub async fn load_page_hash_matches_page(
    database_file: &Path,
    page_hash: &str,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    load_page_hash_matches_page_model(database_file, page_hash, page, size).await
}

pub async fn load_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
    if let Some(bytes) = load_page_hash_thumbnail_model(database_file, page_hash).await? {
        return Ok(Some(PageHashThumbnail {
            bytes,
            media_type: "image/jpeg".to_string(),
        }));
    }

    let Some(source) = load_unknown_page_hash_source(database_file, page_hash).await? else {
        return Ok(None);
    };
    if !source.media_type.starts_with("image/") {
        return Ok(None);
    }

    let source_path = PathBuf::from(source.library_root).join(source.book_url);
    let Some(bytes) = read_unknown_thumbnail_bytes(&source_path, &source.file_name) else {
        return Ok(None);
    };

    Ok(Some(PageHashThumbnail {
        bytes,
        media_type: source.media_type,
    }))
}

pub async fn upsert_page_hash(
    database_file: &Path,
    page_hash: &str,
    size: Option<i64>,
    action: &str,
) -> Result<(), sqlx::Error> {
    upsert_page_hash_model(database_file, page_hash, size, action).await
}

pub async fn delete_all_page_hash_matches(
    database_file: &Path,
    page_hash: &str,
) -> Result<u64, sqlx::Error> {
    delete_all_page_hash_matches_model(database_file, page_hash).await
}

pub async fn delete_page_hash_match(
    database_file: &Path,
    page_hash: &str,
    book_id: &str,
    page_number: u64,
) -> Result<u64, sqlx::Error> {
    delete_page_hash_match_model(database_file, page_hash, book_id, page_number).await
}

fn read_unknown_thumbnail_bytes(source_path: &Path, file_name: &str) -> Option<Vec<u8>> {
    let extension = source_path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(
        extension.as_str(),
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp"
    ) {
        return fs::read(source_path).ok();
    }

    if matches!(extension.as_str(), "cbz" | "zip" | "epub") {
        let file = fs::File::open(source_path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name(file_name).ok()?;
        let mut bytes = Vec::new();
        Read::read_to_end(&mut entry, &mut bytes).ok()?;
        return Some(bytes);
    }

    if matches!(extension.as_str(), "cbr" | "rar") {
        let output = Command::new("unrar")
            .arg("p")
            .arg("-inul")
            .arg(source_path)
            .arg(file_name)
            .output()
            .ok()?;
        if output.status.success() && !output.stdout.is_empty() {
            return Some(output.stdout);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};
    use zip::CompressionMethod;
    use zip::write::SimpleFileOptions;

    async fn create_test_db(case: &str) -> (PathBuf, sqlx::Pool<sqlx::Sqlite>) {
        let root = unique_temp_dir(case);
        fs::create_dir_all(&root).expect("temp root should be created");
        let db_path = root.join("page-hashes.sqlite");
        let pool = crate::sqlite::connect_pool(&db_path, 1)
            .await
            .expect("test db should open");

        sqlx::query(
            "CREATE TABLE IF NOT EXISTS LIBRARY (ID varchar NOT NULL PRIMARY KEY, ROOT varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("library table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS BOOK (ID varchar NOT NULL PRIMARY KEY, URL varchar NOT NULL, LIBRARY_ID varchar NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("book table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS MEDIA_PAGE (BOOK_ID varchar NOT NULL, NUMBER int NOT NULL, FILE_HASH varchar NOT NULL DEFAULT '', FILE_NAME varchar NOT NULL, MEDIA_TYPE varchar NOT NULL, FILE_SIZE int8 NULL, PRIMARY KEY (BOOK_ID, NUMBER))",
        )
        .execute(&pool)
        .await
        .expect("media page table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS PAGE_HASH (HASH varchar NOT NULL PRIMARY KEY, SIZE int8 NULL, ACTION varchar NOT NULL, DELETE_COUNT int NOT NULL DEFAULT 0, CREATED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP, LAST_MODIFIED_DATE datetime NOT NULL DEFAULT CURRENT_TIMESTAMP)",
        )
        .execute(&pool)
        .await
        .expect("page hash table should be created");
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS PAGE_HASH_THUMBNAIL (HASH varchar NOT NULL PRIMARY KEY, THUMBNAIL blob NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("thumbnail table should be created");

        (db_path, pool)
    }

    fn unique_temp_dir(case: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-page-hashes-{case}-{nanos}-{}",
            std::process::id()
        ))
    }

    fn write_zip_archive(path: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(path).expect("zip file should be created");
        let mut writer = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);

        for (name, bytes) in entries {
            writer
                .start_file(name, options)
                .expect("zip entry should start");
            writer
                .write_all(bytes)
                .expect("zip entry bytes should be written");
        }

        writer.finish().expect("zip file should be finished");
    }

    #[tokio::test]
    async fn load_page_hash_thumbnail_returns_persisted_thumbnail_with_jpeg_content_type() {
        let (db_path, pool) = create_test_db("persisted-thumbnail").await;
        let expected = vec![1_u8, 2, 3, 4];

        sqlx::query("INSERT INTO PAGE_HASH_THUMBNAIL (HASH, THUMBNAIL) VALUES (?, ?)")
            .bind("hash-1")
            .bind(&expected)
            .execute(&pool)
            .await
            .expect("thumbnail row should be inserted");

        let thumbnail = load_page_hash_thumbnail(db_path.as_path(), "hash-1")
            .await
            .expect("thumbnail lookup should succeed")
            .expect("thumbnail should exist");

        assert_eq!(thumbnail.bytes, expected);
        assert_eq!(thumbnail.media_type, "image/jpeg");
    }

    #[tokio::test]
    async fn load_page_hash_thumbnail_reads_plain_image_file_when_source_is_image() {
        let (db_path, pool) = create_test_db("plain-image-fallback").await;
        let root = db_path.parent().expect("db path should have a parent");
        let library_root = root.join("library-root");
        let library_root_value = library_root.to_string_lossy().to_string();
        fs::create_dir_all(&library_root).expect("library root should be created");

        let source_path = library_root.join("cover.jpg");
        let expected = vec![9_u8, 8, 7, 6];
        fs::write(&source_path, &expected).expect("source image should be written");

        sqlx::query("INSERT INTO LIBRARY (ID, ROOT) VALUES (?, ?)")
            .bind("library-1")
            .bind(library_root_value.as_str())
            .execute(&pool)
            .await
            .expect("library row should be inserted");
        sqlx::query("INSERT INTO BOOK (ID, URL, LIBRARY_ID) VALUES (?, ?, ?)")
            .bind("book-1")
            .bind("cover.jpg")
            .bind("library-1")
            .execute(&pool)
            .await
            .expect("book row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind(0_i64)
        .bind("hash-2")
        .bind("cover.jpg")
        .bind("image/png")
        .bind(4_i64)
        .execute(&pool)
        .await
        .expect("media page row should be inserted");

        let thumbnail = load_page_hash_thumbnail(db_path.as_path(), "hash-2")
            .await
            .expect("thumbnail lookup should succeed")
            .expect("thumbnail should exist");

        assert_eq!(thumbnail.bytes, expected);
        assert_eq!(thumbnail.media_type, "image/png");
    }

    #[tokio::test]
    async fn load_page_hash_thumbnail_reads_zip_entry_when_source_is_archive() {
        let (db_path, pool) = create_test_db("zip-fallback").await;
        let root = db_path.parent().expect("db path should have a parent");
        let library_root = root.join("library-root");
        let library_root_value = library_root.to_string_lossy().to_string();
        fs::create_dir_all(&library_root).expect("library root should be created");

        let archive_path = library_root.join("book.cbz");
        let expected = vec![5_u8, 4, 3, 2, 1];
        write_zip_archive(&archive_path, &[("cover.png", &expected)]);

        sqlx::query("INSERT INTO LIBRARY (ID, ROOT) VALUES (?, ?)")
            .bind("library-1")
            .bind(library_root_value.as_str())
            .execute(&pool)
            .await
            .expect("library row should be inserted");
        sqlx::query("INSERT INTO BOOK (ID, URL, LIBRARY_ID) VALUES (?, ?, ?)")
            .bind("book-1")
            .bind("book.cbz")
            .bind("library-1")
            .execute(&pool)
            .await
            .expect("book row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-1")
        .bind(0_i64)
        .bind("hash-3")
        .bind("cover.png")
        .bind("image/png")
        .bind(5_i64)
        .execute(&pool)
        .await
        .expect("media page row should be inserted");

        let thumbnail = load_page_hash_thumbnail(db_path.as_path(), "hash-3")
            .await
            .expect("thumbnail lookup should succeed")
            .expect("thumbnail should exist");

        assert_eq!(thumbnail.bytes, expected);
        assert_eq!(thumbnail.media_type, "image/png");
    }
}
