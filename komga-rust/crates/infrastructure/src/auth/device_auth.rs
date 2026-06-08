use std::io::Read;

use flate2::read::GzDecoder;
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::resolve_optional_library_item_path;

pub use komga_application::identity_access::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
};

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

pub async fn load_kobo_metadata_record(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
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
    .fetch_optional(pool)
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
    .fetch_all(pool)
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

pub async fn load_thumbnail_by_id(
    pool: &SqlitePool,
    thumbnail_id: &str,
) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT tb.MEDIA_TYPE, tb.THUMBNAIL, tb.URL, l.ROOT AS LIBRARY_ROOT
 FROM THUMBNAIL_BOOK tb
 LEFT JOIN BOOK b ON b.ID = tb.BOOK_ID
 LEFT JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
 WHERE tb.ID = ?
 LIMIT 1"#,
    )
    .bind(thumbnail_id)
    .fetch_optional(pool)
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

pub async fn persisted_book_exists(pool: &SqlitePool, book_id: &str) -> Result<bool, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT 1 AS FOUND
 FROM BOOK
 WHERE ID = ?
   AND DELETED_DATE IS NULL
 LIMIT 1"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await?;
    Ok(row.is_some())
}

pub async fn load_book_last_epub_position_locator(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<Value>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB
 FROM MEDIA
 WHERE BOOK_ID = ?
 LIMIT 1"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
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
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT CREATED_DATE
 FROM BOOK
 WHERE ID = ?
   AND DELETED_DATE IS NULL
 LIMIT 1"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await?;

    Ok(row
        .map(|row| row.get::<Option<String>, _>("CREATED_DATE"))
        .unwrap_or(None)
        .filter(|value| !value.trim().is_empty()))
}

pub async fn load_read_progress(
    pool: &SqlitePool,
    book_id: &str,
    user_id_value: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
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
    .fetch_optional(pool)
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

pub async fn load_koreader_book_target(
    pool: &SqlitePool,
    book_hash: &str,
) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
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
    .fetch_all(pool)
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
