use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::Value;
use sqlx::Row;

use crate::sqlite::connect_pool;

#[derive(Clone)]
pub struct PersistedBookMediaFile {
    pub file_name: String,
    pub media_type: String,
    pub file_path: PathBuf,
}

#[derive(Clone)]
pub struct PersistedReadProgressRecord {
    pub page: i64,
    pub completed: bool,
    pub created: String,
    pub last_modified: String,
    pub device_id: String,
    pub device_name: String,
    pub locator: Option<Vec<u8>>,
}

#[derive(Clone)]
pub struct KoreaderBookTarget {
    pub id: String,
    pub page_count: u64,
}

#[derive(Clone)]
pub struct KoboMetadataRecord {
    pub title: String,
    pub summary: String,
    pub release_date: Option<String>,
    pub language: String,
    pub file_size: u64,
    pub file_name: String,
}

#[derive(Debug)]
pub enum KoreaderBookLookupError {
    Persistence,
    Conflict,
}

pub async fn load_kobo_metadata_record(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE,\n                COALESCE(bm.SUMMARY, '') AS SUMMARY,\n                bm.RELEASE_DATE AS RELEASE_DATE,\n                COALESCE(sm.LANGUAGE, 'en') AS LANGUAGE,\n                COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,\n                b.NAME AS FILE_NAME\n         FROM BOOK b\n         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID\n         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID\n         WHERE b.ID = ?\n           AND b.DELETED_DATE IS NULL\n         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| KoboMetadataRecord {
        title: row.get::<String, _>("TITLE"),
        summary: row.get::<String, _>("SUMMARY"),
        release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        language: row.get::<String, _>("LANGUAGE"),
        file_size: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        file_name: row.get::<String, _>("FILE_NAME"),
    }))
}

pub async fn load_book_media_file(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT b.NAME AS FILE_NAME,\n                b.URL AS BOOK_URL,\n                l.ROOT AS LIBRARY_ROOT,\n                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE\n         FROM BOOK b\n         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID\n         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID\n         WHERE b.ID = ?\n           AND b.DELETED_DATE IS NULL\n         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let book_url = row.get::<String, _>("BOOK_URL");
        let library_root = row.get::<String, _>("LIBRARY_ROOT");
        PersistedBookMediaFile {
            file_name: file_name.clone(),
            media_type: content_type_from_filename(
                &file_name,
                row.get::<String, _>("MEDIA_TYPE").as_str(),
            ),
            file_path: PathBuf::from(library_root).join(book_url),
        }
    }))
}

pub async fn load_thumbnail_by_id(
    database_file: &Path,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let direct = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL\n         FROM THUMBNAIL_BOOK\n         WHERE ID = ?\n         LIMIT 1",
    )
    .bind(thumbnail_id)
    .fetch_optional(&pool)
    .await?;

    let row = if let Some(row) = direct {
        Some(row)
    } else {
        sqlx::query(
            "SELECT MEDIA_TYPE, THUMBNAIL\n             FROM THUMBNAIL_BOOK\n             WHERE BOOK_ID = ?\n             ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC\n             LIMIT 1",
        )
        .bind(thumbnail_id)
        .fetch_optional(&pool)
        .await?
    };

    Ok(row.map(|row| {
        (
            row.get::<String, _>("MEDIA_TYPE"),
            row.get::<Vec<u8>, _>("THUMBNAIL"),
        )
    }))
}

pub async fn persisted_book_exists(
    database_file: &Path,
    book_id: &str,
) -> Result<bool, sqlx::Error> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT 1 AS FOUND\n         FROM BOOK\n         WHERE ID = ?\n           AND DELETED_DATE IS NULL\n         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;
    Ok(row.is_some())
}

pub async fn load_book_page_count(database_file: &Path, book_id: &str) -> Result<u64, sqlx::Error> {
    if !database_file.exists() {
        return Ok(1);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COALESCE(PAGE_COUNT, 0) AS PAGE_COUNT\n         FROM MEDIA\n         WHERE BOOK_ID = ?\n         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row
        .map(|row| row.get::<i64, _>("PAGE_COUNT").max(0) as u64)
        .unwrap_or(1)
        .max(1))
}

pub async fn load_book_last_epub_position_locator(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB\n         FROM MEDIA\n         WHERE BOOK_ID = ?\n         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let extension_class = row
        .get::<Option<String>, _>("EXTENSION_CLASS")
        .unwrap_or_default();
    if !extension_class.is_empty()
        && !extension_class
            .to_ascii_lowercase()
            .contains("mediaextensionepub")
    {
        return Ok(None);
    }

    let Some(blob) = row.get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB") else {
        return Ok(None);
    };

    let mut decoder = GzDecoder::new(blob.as_slice());
    let mut decoded = Vec::new();
    if decoder.read_to_end(&mut decoded).is_err() {
        return Ok(None);
    }

    let extension_json = match serde_json::from_slice::<Value>(&decoded) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };

    Ok(extension_json
        .get("positions")
        .and_then(Value::as_array)
        .and_then(|positions| positions.last().cloned()))
}

pub async fn load_book_created_timestamp(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT CREATED_DATE\n         FROM BOOK\n         WHERE ID = ?\n           AND DELETED_DATE IS NULL\n         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row
        .map(|row| row.get::<Option<String>, _>("CREATED_DATE"))
        .unwrap_or(None)
        .filter(|value| !value.trim().is_empty()))
}

pub async fn load_read_progress(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT PAGE, COMPLETED, CREATED_DATE, LAST_MODIFIED_DATE,\n                COALESCE(DEVICE_ID, '') AS DEVICE_ID,\n                COALESCE(DEVICE_NAME, '') AS DEVICE_NAME,\n                LOCATOR\n         FROM READ_PROGRESS\n         WHERE BOOK_ID = ?\n           AND USER_ID = ?\n         LIMIT 1",
    )
    .bind(book_id)
    .bind(user_id_value)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedReadProgressRecord {
        page: row.get::<i64, _>("PAGE"),
        completed: row.get::<bool, _>("COMPLETED"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        device_id: row.get::<String, _>("DEVICE_ID"),
        device_name: row.get::<String, _>("DEVICE_NAME"),
        locator: row
            .try_get::<Option<Vec<u8>>, _>("LOCATOR")
            .or_else(|_| row.try_get::<Option<Vec<u8>>, _>("locator"))
            .ok()
            .flatten(),
    }))
}

pub async fn persist_read_progress_with_locator(
    database_file: &Path,
    book_id: &str,
    user_id_value: &str,
    page: i64,
    completed: bool,
    device_id: &str,
    device_name: &str,
    last_modified: &str,
    locator: Option<Value>,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open read-progress db: {error}"))?;

    let user_exists =
        sqlx::query("SELECT 1\n         FROM USER\n         WHERE ID = ?\n         LIMIT 1")
            .bind(user_id_value)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("query read-progress user: {error}"))?
            .is_some();

    if !user_exists {
        return Err("read-progress user not found".to_string());
    }

    let locator_blob = locator
        .and_then(|value| serde_json::to_vec(&value).ok())
        .unwrap_or_default();

    sqlx::query(
        "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, DEVICE_ID, DEVICE_NAME,\n                                   LAST_MODIFIED_DATE, LOCATOR)\n         VALUES (?, ?, ?, ?, ?, ?, ?, ?)\n         ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE\n         SET PAGE = excluded.PAGE,\n             COMPLETED = excluded.COMPLETED,\n             DEVICE_ID = excluded.DEVICE_ID,\n             DEVICE_NAME = excluded.DEVICE_NAME,\n             LOCATOR = excluded.LOCATOR,\n             LAST_MODIFIED_DATE = excluded.LAST_MODIFIED_DATE",
    )
    .bind(book_id)
    .bind(user_id_value)
    .bind(page.max(0))
    .bind(completed)
    .bind(device_id)
    .bind(device_name)
    .bind(last_modified)
    .bind(locator_blob)
    .execute(&pool)
    .await
    .map_err(|error| format!("persist read-progress with locator: {error}"))?;

    Ok(())
}

pub async fn load_koreader_book_target(
    database_file: &Path,
    book_hash: &str,
) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|_| KoreaderBookLookupError::Persistence)?;
    let rows = sqlx::query(
        "SELECT b.ID AS BOOK_ID, COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT\n         FROM BOOK b\n         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID\n         WHERE b.FILE_HASH_KOREADER = ?\n           AND b.DELETED_DATE IS NULL\n         ORDER BY b.ID ASC",
    )
    .bind(book_hash)
    .fetch_all(&pool)
    .await
    .map_err(|_| KoreaderBookLookupError::Persistence)?;

    if rows.is_empty() {
        return Ok(None);
    }
    if rows.len() > 1 {
        return Err(KoreaderBookLookupError::Conflict);
    }

    let row = rows.first().expect("koreader target row should exist");
    Ok(Some(KoreaderBookTarget {
        id: row.get::<String, _>("BOOK_ID"),
        page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
    }))
}

fn content_type_from_filename(file_name: &str, default_mime_type: &str) -> String {
    match file_name
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("cbz") => "application/vnd.comicbook+zip".to_string(),
        Some("cbr") => "application/vnd.comicbook-rar".to_string(),
        Some("epub") => "application/epub+zip".to_string(),
        Some("pdf") => "application/pdf".to_string(),
        Some("jpg") | Some("jpeg") => "image/jpeg".to_string(),
        Some("png") => "image/png".to_string(),
        Some("gif") => "image/gif".to_string(),
        Some("webp") => "image/webp".to_string(),
        Some("avif") => "image/avif".to_string(),
        _ => default_mime_type.to_string(),
    }
}
