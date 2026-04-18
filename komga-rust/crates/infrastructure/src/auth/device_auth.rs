#![allow(clippy::too_many_arguments)]

use std::io::Read;
use std::path::{Path, PathBuf};

use flate2::read::GzDecoder;
use serde_json::Value;
use sqlx::Row;

use crate::sqlite::{connect_read_pool, connect_write_pool};
use crate::{resolve_library_item_path, resolve_optional_library_item_path};

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
    pub media_type: String,
}

#[derive(Clone)]
pub struct KoboMetadataRecord {
    pub title: String,
    pub summary: String,
    pub release_date: Option<String>,
    pub created_date: Option<String>,
    pub language: String,
    pub file_size: u64,
    pub file_name: String,
    pub media_type: String,
    pub contributor_names: Vec<String>,
    pub isbn: Option<String>,
    pub publisher_name: Option<String>,
    pub cover_image_id: Option<String>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub series_number: Option<String>,
    pub series_number_float: Option<f64>,
    pub oneshot: bool,
    pub is_kepub: bool,
    pub is_pre_paginated: bool,
}

fn decode_epub_extension_is_fixed_layout(blob: &[u8]) -> bool {
    let mut decoder = GzDecoder::new(blob);
    let mut json = String::new();
    if decoder.read_to_string(&mut json).is_err() {
        return false;
    }
    serde_json::from_str::<Value>(&json)
        .ok()
        .and_then(|value| value.get("isFixedLayout").and_then(Value::as_bool))
        .unwrap_or(false)
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

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT COALESCE(bm.TITLE, b.NAME) AS TITLE,
       COALESCE(bm.SUMMARY, '') AS SUMMARY,
       bm.RELEASE_DATE AS RELEASE_DATE,
       COALESCE(bm.CREATED_DATE, b.CREATED_DATE, '') AS CREATED_DATE,
       COALESCE(sm.LANGUAGE, 'en') AS LANGUAGE,
       COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
       b.NAME AS FILE_NAME,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
       NULLIF(TRIM(bm.ISBN), '') AS ISBN,
       NULLIF(TRIM(sm.PUBLISHER), '') AS PUBLISHER_NAME,
       tb.ID AS COVER_IMAGE_ID,
       sm.SERIES_ID AS SERIES_ID,
       sm.TITLE AS SERIES_NAME,
       NULLIF(TRIM(bm.NUMBER), '') AS SERIES_NUMBER,
       bm.NUMBER_SORT AS SERIES_NUMBER_FLOAT,
       COALESCE(b.ONESHOT, FALSE) AS ONESHOT,
       COALESCE(m.EPUB_IS_KEPUB, FALSE) AS EPUB_IS_KEPUB,
       m.EXTENSION_VALUE_BLOB AS EXTENSION_VALUE_BLOB
 FROM BOOK b
  LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
  LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = b.SERIES_ID
  LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
  LEFT JOIN THUMBNAIL_BOOK tb ON tb.BOOK_ID = b.ID AND tb.SELECTED = TRUE
 WHERE b.ID = ?
   AND b.DELETED_DATE IS NULL
   AND bm.BOOK_ID IS NOT NULL
 LIMIT 1"#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;

    let contributor_rows = sqlx::query(
        r#"SELECT NAME
 FROM BOOK_METADATA_AUTHOR
 WHERE BOOK_ID = ?
   AND NAME IS NOT NULL
   AND TRIM(NAME) <> ''
 ORDER BY NAME ASC"#,
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await?;

    let contributor_names = contributor_rows
        .into_iter()
        .map(|row| row.get::<String, _>("NAME"))
        .collect::<Vec<_>>();

    Ok(row.map(|row| KoboMetadataRecord {
        title: row.get::<String, _>("TITLE"),
        summary: row.get::<String, _>("SUMMARY"),
        release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        created_date: {
            let created_date = row.get::<String, _>("CREATED_DATE");
            let created_date = created_date.trim();
            if created_date.is_empty() {
                None
            } else {
                Some(created_date.to_string())
            }
        },
        language: row.get::<String, _>("LANGUAGE"),
        file_size: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        contributor_names,
        isbn: row.get::<Option<String>, _>("ISBN"),
        publisher_name: row.get::<Option<String>, _>("PUBLISHER_NAME"),
        cover_image_id: row.get::<Option<String>, _>("COVER_IMAGE_ID"),
        series_id: row.get::<Option<String>, _>("SERIES_ID"),
        series_name: row.get::<Option<String>, _>("SERIES_NAME"),
        series_number: row.get::<Option<String>, _>("SERIES_NUMBER"),
        series_number_float: row.get::<Option<f64>, _>("SERIES_NUMBER_FLOAT"),
        oneshot: row.get::<bool, _>("ONESHOT"),
        is_kepub: row.get::<bool, _>("EPUB_IS_KEPUB"),
        is_pre_paginated: row
            .get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB")
            .as_deref()
            .is_some_and(decode_epub_extension_is_fixed_layout),
    }))
}

pub async fn load_book_media_file(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT,
       COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE
 FROM BOOK b
 JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
 LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
 WHERE b.ID = ?
   AND b.DELETED_DATE IS NULL
 LIMIT 1"#,
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
            file_path: resolve_library_item_path(library_root.as_str(), book_url.as_str()),
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

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT tb.MEDIA_TYPE, tb.THUMBNAIL, tb.URL, l.ROOT AS LIBRARY_ROOT
 FROM THUMBNAIL_BOOK tb
 LEFT JOIN BOOK b ON b.ID = tb.BOOK_ID
 LEFT JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
 WHERE tb.ID = ?
 LIMIT 1"#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let media_type = row.get::<String, _>("MEDIA_TYPE");
    if let Some(thumbnail) = row.get::<Option<Vec<u8>>, _>("THUMBNAIL") {
        return Ok(Some((media_type, thumbnail)));
    }

    let Some(url) = row.get::<Option<String>, _>("URL") else {
        return Ok(None);
    };
    let library_root = row.get::<Option<String>, _>("LIBRARY_ROOT");
    let sidecar_path = resolve_optional_library_item_path(library_root.as_deref(), &url);
    let Some(sidecar_path) = sidecar_path else {
        return Ok(None);
    };

    match std::fs::read(&sidecar_path) {
        Ok(bytes) => Ok(Some((media_type, bytes))),
        Err(_) => Ok(None),
    }
}

pub async fn persisted_book_exists(
    database_file: &Path,
    book_id: &str,
) -> Result<bool, sqlx::Error> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT 1 AS FOUND
 FROM BOOK
 WHERE ID = ?
   AND DELETED_DATE IS NULL
 LIMIT 1"#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await?;
    Ok(row.is_some())
}

pub async fn load_book_last_epub_position_locator(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB
 FROM MEDIA
 WHERE BOOK_ID = ?
 LIMIT 1"#,
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

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT CREATED_DATE
 FROM BOOK
 WHERE ID = ?
   AND DELETED_DATE IS NULL
 LIMIT 1"#,
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

    let pool = connect_read_pool(database_file).await?;
    let row = sqlx::query(
        r#"SELECT PAGE, COMPLETED, CREATED_DATE, LAST_MODIFIED_DATE,
       COALESCE(DEVICE_ID, '') AS DEVICE_ID,
       COALESCE(DEVICE_NAME, '') AS DEVICE_NAME,
       LOCATOR
 FROM READ_PROGRESS
 WHERE BOOK_ID = ?
   AND USER_ID = ?
 LIMIT 1"#,
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
    let pool = connect_write_pool(database_file)
        .await
        .map_err(|error| format!("open read-progress db: {error}"))?;

    let user_exists = sqlx::query(
        r#"SELECT 1
 FROM USER
 WHERE ID = ?
 LIMIT 1"#,
    )
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
        r#"INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, DEVICE_ID, DEVICE_NAME,
                           LAST_MODIFIED_DATE, LOCATOR)
 VALUES (?, ?, ?, ?, ?, ?, ?, ?)
 ON CONFLICT(BOOK_ID, USER_ID) DO UPDATE
 SET PAGE = excluded.PAGE,
     COMPLETED = excluded.COMPLETED,
     DEVICE_ID = excluded.DEVICE_ID,
     DEVICE_NAME = excluded.DEVICE_NAME,
     LOCATOR = excluded.LOCATOR,
     LAST_MODIFIED_DATE = excluded.LAST_MODIFIED_DATE"#,
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

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|_| KoreaderBookLookupError::Persistence)?;
    let rows = sqlx::query(
        r#"SELECT b.ID AS BOOK_ID,
         COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
         COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE
  FROM BOOK b
  LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
  WHERE b.FILE_HASH_KOREADER = ?
   AND b.DELETED_DATE IS NULL
 ORDER BY b.ID ASC"#,
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
        media_type: row.get::<String, _>("MEDIA_TYPE"),
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
