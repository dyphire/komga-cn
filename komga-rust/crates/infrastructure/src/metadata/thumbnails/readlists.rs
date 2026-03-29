use std::path::Path;

use komga_application::media_assets::ReadlistThumbnailRecord;
use sqlx::Row;

use crate::sqlite::connect_pool;

use super::generated_thumbnail_id;

pub async fn load_persisted_readlist_thumbnails(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<ReadlistThumbnailRecord>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnails db: {error}"))?;
    let rows = sqlx::query(
        "SELECT ID, TYPE, SELECTED, MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_READLIST \
         WHERE READLIST_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted readlist thumbnails: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| ReadlistThumbnailRecord {
            id: row.get::<String, _>("ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
        })
        .collect())
}

pub async fn insert_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    selected: bool,
) -> Result<ReadlistThumbnailRecord, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM READLIST \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail create tx: {error}"))?;
        return Err("readlist does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_READLIST \
             SET SELECTED = 0 \
             WHERE READLIST_ID = ?",
        )
        .bind(readlist_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected readlist thumbnails: {error}"))?;
    }

    let id = generated_thumbnail_id("thumbnail-readlist");
    sqlx::query(
        "INSERT INTO THUMBNAIL_READLIST \
         (ID, SELECTED, THUMBNAIL, TYPE, READLIST_ID, MEDIA_TYPE, FILE_SIZE) \
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(readlist_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert readlist thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail create tx: {error}"))?;

    Ok(ReadlistThumbnailRecord {
        id,
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        thumbnail: thumbnail.to_vec(),
    })
}

pub async fn select_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM READLIST \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM THUMBNAIL_READLIST \
         WHERE ID = ? AND READLIST_ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist thumbnail select target: {error}"))?
    .is_some();
    if !target_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE THUMBNAIL_READLIST \
         SET SELECTED = 0 \
         WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected readlist thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_READLIST \
         SET SELECTED = 1 \
         WHERE ID = ? AND READLIST_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected readlist thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail select tx: {error}"))?;
    Ok(true)
}

pub async fn delete_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM READLIST \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE FROM THUMBNAIL_READLIST \
         WHERE ID = ? AND READLIST_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete readlist thumbnail: {error}"))?
    .rows_affected()
        > 0;
    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail delete tx: {error}"))?;
    Ok(true)
}

pub async fn load_persisted_readlist_name(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist file db: {error}"))?;
    let row = sqlx::query(
        "SELECT NAME \
         FROM READLIST \
         WHERE ID = ?",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted readlist name: {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("NAME")))
}

pub async fn persisted_readlist_exists(
    database_file: &Path,
    readlist_id: &str,
) -> Result<bool, String> {
    Ok(load_persisted_readlist_name(database_file, readlist_id)
        .await?
        .is_some())
}
