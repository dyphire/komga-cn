use std::fs;
use std::io::Read;
use std::path::Path;

use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, book_media_is_pdf, book_media_is_single_image,
    content_type_from_filename,
};
use serde_json::Value;
use std::io::Cursor;
use zip::ZipArchive;

use crate::filesystem::{
    load_archive_page_row, load_pdf_page_row, load_persisted_book_media,
    load_persisted_book_page_row, render_book_page_thumbnail, resolve_book_page_bytes,
};
use crate::rar_support::read_rar_entry_bytes;
use crate::resolve_library_item_path;
use crate::sqlite::connect_pool;
use crate::sqlite::read_models::{
    load_page_hash_delete_targets as load_page_hash_delete_targets_model,
    load_page_hash_matches_page as load_page_hash_matches_page_model,
    load_page_hash_thumbnail as load_page_hash_thumbnail_model,
    load_page_hashes_page as load_page_hashes_page_model,
    load_page_hashes_unknown_page as load_page_hashes_unknown_page_model,
    load_unknown_page_hash_match_target, load_unknown_page_hash_source,
};
use crate::sqlite::write_models::upsert_page_hash as upsert_page_hash_model;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashThumbnail {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashDeleteTargetPage {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashDeleteTarget {
    pub book_id: String,
    pub pages: Vec<PageHashDeleteTargetPage>,
}

const KOTLIN_PDF_MIN_EDGE: u32 = 3200;

pub async fn load_page_hashes_page(
    database_file: &Path,
    page: u64,
    size: u64,
    actions: &[String],
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    load_page_hashes_page_model(database_file, page, size, actions, sorts).await
}

pub async fn load_page_hashes_unknown_page(
    database_file: &Path,
    page: u64,
    size: u64,
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    load_page_hashes_unknown_page_model(database_file, page, size, sorts).await
}

pub async fn load_page_hash_matches_page(
    database_file: &Path,
    page_hash: &str,
    page: u64,
    size: u64,
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    load_page_hash_matches_page_model(database_file, page_hash, page, size, sorts).await
}

pub async fn load_page_hash_delete_targets(
    database_file: &Path,
    page_hash: &str,
) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
    load_page_hash_delete_targets_model(database_file, page_hash)
        .await
        .map(|targets| {
            targets
                .into_iter()
                .map(|target| PageHashDeleteTarget {
                    book_id: target.book_id,
                    pages: target
                        .pages
                        .into_iter()
                        .map(|page| PageHashDeleteTargetPage {
                            file_hash: page.file_hash,
                            file_size: page.file_size,
                            file_name: page.file_name,
                            media_type: page.media_type,
                            page_number: page.page_number,
                        })
                        .collect(),
                })
                .collect()
        })
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

    let Some((bytes, media_type)) =
        load_unknown_page_hash_source_bytes(database_file, page_hash).await?
    else {
        return Ok(None);
    };

    Ok(Some(PageHashThumbnail { bytes, media_type }))
}

pub async fn load_unknown_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
    resize_to: Option<u32>,
) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
    let Some(target) = load_unknown_page_hash_match_target(database_file, page_hash).await? else {
        return Ok(None);
    };

    let Some(media) = load_persisted_book_media(database_file, &target.book_id)
        .await
        .map_err(as_sqlx_protocol_error)?
    else {
        return Ok(None);
    };

    let Some(page) =
        load_page_hash_page_row(database_file, &target.book_id, &media, target.page_number)
            .await
            .map_err(as_sqlx_protocol_error)?
    else {
        return Ok(None);
    };

    if let Some(max_edge) = resize_to {
        let Some(bytes) = render_book_page_thumbnail(&media, &page, target.page_number, max_edge)
        else {
            return Ok(None);
        };

        return Ok(Some(PageHashThumbnail {
            bytes,
            media_type: "image/jpeg".to_string(),
        }));
    }

    if book_media_is_pdf(&media) {
        let Some(bytes) = render_book_page_thumbnail(
            &media,
            &page,
            target.page_number,
            pdf_render_max_edge(&page),
        ) else {
            return Ok(None);
        };

        return Ok(Some(PageHashThumbnail {
            media_type: "image/jpeg".to_string(),
            bytes,
        }));
    }

    let Some(bytes) = resolve_book_page_bytes(&media, &page, target.page_number) else {
        return Ok(None);
    };

    Ok(Some(PageHashThumbnail {
        media_type: page_media_type(&page, &media),
        bytes,
    }))
}

pub async fn upsert_page_hash(
    database_file: &Path,
    page_hash: &str,
    size: Option<i64>,
    action: &str,
) -> Result<(), sqlx::Error> {
    let existed = page_hash_exists(database_file, page_hash).await?;
    upsert_page_hash_model(database_file, page_hash, size, action).await?;

    if !existed
        && let Some(thumbnail) = build_known_page_hash_thumbnail(database_file, page_hash).await?
    {
        insert_page_hash_thumbnail(database_file, page_hash, &thumbnail).await?;
    }

    Ok(())
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
        return read_rar_entry_bytes(source_path, file_name).ok().flatten();
    }

    None
}

async fn load_page_hash_page_row(
    database_file: &Path,
    book_id: &str,
    media: &BookMediaRecord,
    page_number: u64,
) -> Result<Option<BookPageRecord>, String> {
    if let Some(row) = load_persisted_book_page_row(database_file, book_id, page_number).await? {
        return Ok(Some(row));
    }

    if book_media_is_single_image(media) && page_number == 1 {
        return Ok(Some(single_image_page_row(media, page_number)));
    }

    Ok(load_archive_page_row(media, page_number).or_else(|| load_pdf_page_row(media, page_number)))
}

fn single_image_page_row(media: &BookMediaRecord, page_number: u64) -> BookPageRecord {
    BookPageRecord {
        number: page_number,
        file_name: media.file_name.clone(),
        media_type: content_type_from_filename(&media.file_name, &media.media_type),
        width: None,
        height: None,
        file_size: fs::metadata(&media.file_path)
            .ok()
            .and_then(|metadata| i64::try_from(metadata.len()).ok())
            .unwrap_or(0),
    }
}

fn page_media_type(page: &BookPageRecord, media: &BookMediaRecord) -> String {
    if page.media_type.is_empty() {
        content_type_from_filename(&page.file_name, &media.media_type)
    } else {
        page.media_type.clone()
    }
}

fn pdf_render_max_edge(page: &BookPageRecord) -> u32 {
    let width = page.width.unwrap_or_default().max(0) as u32;
    let height = page.height.unwrap_or_default().max(0) as u32;
    width.max(height).max(KOTLIN_PDF_MIN_EDGE)
}

fn as_sqlx_protocol_error(error: String) -> sqlx::Error {
    sqlx::Error::Protocol(error)
}

async fn page_hash_exists(database_file: &Path, page_hash: &str) -> Result<bool, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let exists = sqlx::query_scalar::<_, i64>("SELECT 1 FROM PAGE_HASH WHERE HASH = ? LIMIT 1")
        .bind(page_hash)
        .fetch_optional(&pool)
        .await?
        .is_some();
    Ok(exists)
}

async fn build_known_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let Some((bytes, _)) = load_unknown_page_hash_source_bytes(database_file, page_hash).await?
    else {
        return Ok(None);
    };

    Ok(encode_image_bytes_as_thumbnail_jpeg(&bytes, 500))
}

async fn load_unknown_page_hash_source_bytes(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<(Vec<u8>, String)>, sqlx::Error> {
    let Some(source) = load_unknown_page_hash_source(database_file, page_hash).await? else {
        return Ok(None);
    };
    if !source.media_type.starts_with("image/") {
        return Ok(None);
    }

    let source_path =
        resolve_library_item_path(source.library_root.as_str(), source.book_url.as_str());
    Ok(
        read_unknown_thumbnail_bytes(&source_path, &source.file_name)
            .map(|bytes| (bytes, source.media_type)),
    )
}

async fn insert_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
    thumbnail: &[u8],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    sqlx::query("INSERT INTO PAGE_HASH_THUMBNAIL (HASH, THUMBNAIL) VALUES (?, ?)")
        .bind(page_hash)
        .bind(thumbnail)
        .execute(&pool)
        .await?;
    Ok(())
}

fn encode_image_bytes_as_thumbnail_jpeg(bytes: &[u8], max_edge: u32) -> Option<Vec<u8>> {
    let image = image::load_from_memory(bytes).ok()?;
    let resized = image.thumbnail(max_edge.max(1), max_edge.max(1));
    let mut output = Cursor::new(Vec::new());
    resized
        .write_to(&mut output, image::ImageFormat::Jpeg)
        .ok()?;
    Some(output.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::setup;
    use std::fs::{self, File};
    use std::io::Write;
    use std::path::PathBuf;
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
        setup::bootstrap_pool(&pool)
            .await
            .expect("test db should bootstrap main schema");

        (db_path, pool)
    }

    async fn insert_library_and_series(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        library_id: &str,
        library_root: &str,
        series_id: &str,
        series_url: &str,
    ) {
        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind(library_id)
            .bind("Library")
            .bind(library_root)
            .execute(pool)
            .await
            .expect("library row should be inserted");
        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)",
        )
        .bind(series_id)
        .bind(0_i64)
        .bind("Series")
        .bind(series_url)
        .bind(library_id)
        .bind(0)
        .execute(pool)
        .await
        .expect("series row should be inserted");
    }

    async fn insert_book(
        pool: &sqlx::Pool<sqlx::Sqlite>,
        book_id: &str,
        book_name: &str,
        book_url: &str,
        series_id: &str,
        library_id: &str,
    ) {
        sqlx::query(
            r#"
            INSERT INTO BOOK (
                ID,
                FILE_LAST_MODIFIED,
                NAME,
                URL,
                SERIES_ID,
                FILE_SIZE,
                NUMBER,
                LIBRARY_ID
            ) VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?)
            "#,

        )
        .bind(book_id)
        .bind(0_i64)
        .bind(book_name)
        .bind(book_url)
        .bind(series_id)
        .bind(0_i64)
        .bind(1_i64)
        .bind(library_id)
        .execute(pool)
        .await
        .expect("book row should be inserted");
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

    fn legacy_file_url(path: &Path) -> String {
        format!("file:{}", path.to_string_lossy().replace(' ', "%20"))
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

        insert_library_and_series(
            &pool,
            "library-1",
            library_root_value.as_str(),
            "series-1",
            "series-1",
        )
        .await;
        insert_book(
            &pool,
            "book-1",
            "cover.jpg",
            "cover.jpg",
            "series-1",
            "library-1",
        )
        .await;
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

        insert_library_and_series(
            &pool,
            "library-1",
            library_root_value.as_str(),
            "series-1",
            "series-1",
        )
        .await;
        insert_book(
            &pool,
            "book-1",
            "book.cbz",
            "book.cbz",
            "series-1",
            "library-1",
        )
        .await;
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

    #[tokio::test]
    async fn load_page_hash_thumbnail_reads_zip_entry_when_source_uses_legacy_file_urls() {
        let (db_path, pool) = create_test_db("zip-fallback-legacy-file-url").await;
        let root = db_path.parent().expect("db path should have a parent");
        let library_root = root.join("library root");
        fs::create_dir_all(&library_root).expect("legacy library root should be created");

        let archive_path = library_root.join("book with spaces.cbz");
        let expected = vec![7_u8, 7, 4, 2];
        write_zip_archive(&archive_path, &[("cover.png", &expected)]);

        let legacy_library_root = legacy_file_url(&library_root);
        let legacy_book_url = legacy_file_url(&archive_path);
        insert_library_and_series(
            &pool,
            "library-1",
            legacy_library_root.as_str(),
            "series-legacy",
            "series-legacy",
        )
        .await;
        insert_book(
            &pool,
            "book-legacy",
            "book with spaces.cbz",
            legacy_book_url.as_str(),
            "series-legacy",
            "library-1",
        )
        .await;
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("book-legacy")
        .bind(0_i64)
        .bind("hash-legacy")
        .bind("cover.png")
        .bind("image/png")
        .bind(4_i64)
        .execute(&pool)
        .await
        .expect("legacy media page row should be inserted");

        let thumbnail = load_page_hash_thumbnail(db_path.as_path(), "hash-legacy")
            .await
            .expect("legacy thumbnail lookup should succeed")
            .expect("legacy thumbnail should exist");

        assert_eq!(thumbnail.bytes, expected);
        assert_eq!(thumbnail.media_type, "image/png");
    }
}
