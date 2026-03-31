use std::path::Path;

use komga_application::media_assets::CollectionThumbnailRecord;
use sqlx::Row;

use crate::sqlite::connect_pool;

use super::generated_thumbnail_id;

pub async fn persisted_collection_exists(
    database_file: &Path,
    collection_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection exists db: {error}"))?;
    let row = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM COLLECTION \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted collection existence: {error}"))?;
    Ok(row.is_some())
}

pub async fn load_persisted_collection_thumbnails(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<CollectionThumbnailRecord>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnails db: {error}"))?;
    let rows = sqlx::query(
        "SELECT ID, COLLECTION_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, THUMBNAIL \
         FROM THUMBNAIL_COLLECTION \
         WHERE COLLECTION_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC",
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted collection thumbnails: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| CollectionThumbnailRecord {
            id: row.get::<String, _>("ID"),
            collection_id: row.get::<String, _>("COLLECTION_ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            width: row.get::<i64, _>("WIDTH"),
            height: row.get::<i64, _>("HEIGHT"),
            thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
        })
        .collect())
}

pub async fn insert_collection_thumbnail(
    database_file: &Path,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<CollectionThumbnailRecord, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM COLLECTION \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail create tx: {error}"))?;
        return Err("collection does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_COLLECTION \
             SET SELECTED = 0 \
             WHERE COLLECTION_ID = ?",
        )
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected collection thumbnails: {error}"))?;
    }

    let id = generated_thumbnail_id("thumbnail-collection");
    sqlx::query(
        "INSERT INTO THUMBNAIL_COLLECTION \
         (ID, SELECTED, THUMBNAIL, TYPE, COLLECTION_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(collection_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .bind(width)
    .bind(height)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert collection thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail create tx: {error}"))?;

    Ok(CollectionThumbnailRecord {
        id,
        collection_id: collection_id.to_string(),
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
        thumbnail: thumbnail.to_vec(),
    })
}

pub async fn select_collection_thumbnail(
    database_file: &Path,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM COLLECTION \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM THUMBNAIL_COLLECTION \
         WHERE ID = ? AND COLLECTION_ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target collection thumbnail for select: {error}"))?
    .is_some();
    if !target_exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    sqlx::query(
        "UPDATE THUMBNAIL_COLLECTION \
         SET SELECTED = 0 \
         WHERE COLLECTION_ID = ?",
    )
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected collection thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_COLLECTION \
         SET SELECTED = 1 \
         WHERE ID = ? AND COLLECTION_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected collection thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail select tx: {error}"))?;
    Ok(true)
}

pub async fn delete_collection_thumbnail(
    database_file: &Path,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM COLLECTION \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE FROM THUMBNAIL_COLLECTION \
         WHERE ID = ? AND COLLECTION_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete collection thumbnail: {error}"))?
    .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail delete tx: {error}"))?;
    Ok(true)
}
