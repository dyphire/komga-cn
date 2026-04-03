use std::path::Path;

use komga_application::media_assets::{EntityThumbnailBinary, EntityThumbnailRecord};
use sqlx::Row;

use crate::sqlite::connect_pool;

use super::generated_thumbnail_id;

pub async fn load_persisted_book_thumbnails(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<EntityThumbnailRecord>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnails db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, BOOK_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT \
         FROM THUMBNAIL_BOOK \
         WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted book thumbnails: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| EntityThumbnailRecord {
            id: row.get::<String, _>("ID"),
            book_id: row.get::<String, _>("BOOK_ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            width: row.get::<i64, _>("WIDTH"),
            height: row.get::<i64, _>("HEIGHT"),
        })
        .collect())
}

pub async fn load_selected_book_thumbnail(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open selected book thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_BOOK \
         WHERE BOOK_ID = ? \
         ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query selected book thumbnail: {error}"))?;

    Ok(row.map(|row| EntityThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

pub async fn load_book_thumbnail_by_id(
    database_file: &Path,
    thumbnail_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open single book thumbnail db: {error}"))?;

    let row = sqlx::query(
        "SELECT MEDIA_TYPE, THUMBNAIL \
         FROM THUMBNAIL_BOOK \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query single book thumbnail: {error}"))?;

    Ok(row.map(|row| EntityThumbnailBinary {
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

pub async fn insert_book_thumbnail(
    database_file: &Path,
    book_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<EntityThumbnailRecord, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM BOOK \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail create tx: {error}"))?;
        return Err("book does not exist".to_string());
    }

    if selected {
        sqlx::query(
            "UPDATE THUMBNAIL_BOOK \
             SET SELECTED = 0 \
             WHERE BOOK_ID = ?",
        )
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected book thumbnails: {error}"))?;
    }

    let id = generated_thumbnail_id("thumbnail-book");
    sqlx::query(
        "INSERT INTO THUMBNAIL_BOOK \
         (ID, SELECTED, THUMBNAIL, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(book_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .bind(width)
    .bind(height)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert book thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail create tx: {error}"))?;

    Ok(EntityThumbnailRecord {
        id,
        book_id: book_id.to_string(),
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
    })
}

pub async fn select_book_thumbnail(
    database_file: &Path,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail select tx: {error}"))?;

    let target_book_id = sqlx::query(
        "SELECT BOOK_ID \
         FROM THUMBNAIL_BOOK \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target book thumbnail for select: {error}"))?
    .map(|row| row.get::<String, _>("BOOK_ID"));
    let Some(target_book_id) = target_book_id else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail select tx: {error}"))?;
        return Ok(false);
    };

    sqlx::query(
        "UPDATE THUMBNAIL_BOOK \
         SET SELECTED = 0 \
         WHERE BOOK_ID = ?",
    )
    .bind(&target_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected book thumbnails for select: {error}"))?;
    sqlx::query(
        "UPDATE THUMBNAIL_BOOK \
         SET SELECTED = 1 \
         WHERE ID = ? AND BOOK_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(target_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected book thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail select tx: {error}"))?;
    Ok(true)
}

pub async fn delete_book_thumbnail(
    database_file: &Path,
    book_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM BOOK \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let deleted = sqlx::query(
        "DELETE FROM THUMBNAIL_BOOK \
         WHERE ID = ? AND BOOK_ID = ?",
    )
    .bind(thumbnail_id)
    .bind(book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete book thumbnail: {error}"))?
    .rows_affected()
        > 0;
    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail delete tx: {error}"))?;
    Ok(true)
}
