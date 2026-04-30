use std::path::Path;

use komga_application::media_assets::ReadlistThumbnailRecord;
use sqlx::Row;

use crate::sqlite::connect_read_pool;

use super::{emit_thumbnail_readlist_event, generated_thumbnail_id};

pub async fn load_persisted_readlist_thumbnails(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<ReadlistThumbnailRecord>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist thumbnails db: {error}"))?;
    let rows = sqlx::query(
        r#"
        SELECT ID, READLIST_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, THUMBNAIL
        FROM THUMBNAIL_READLIST
        WHERE READLIST_ID = ?
        ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC
        "#,
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted readlist thumbnails: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| ReadlistThumbnailRecord {
            id: row.get::<String, _>("ID"),
            readlist_id: row.get::<String, _>("READLIST_ID"),
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

pub async fn insert_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<ReadlistThumbnailRecord, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist thumbnail create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM READLIST
        WHERE ID = ?
        LIMIT 1
        "#,
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
            r#"
            UPDATE THUMBNAIL_READLIST
            SET SELECTED = 0
            WHERE READLIST_ID = ?
            "#,
        )
        .bind(readlist_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected readlist thumbnails: {error}"))?;
    }

    let id = generated_thumbnail_id("thumbnail-readlist");
    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_READLIST
            (ID, SELECTED, THUMBNAIL, TYPE, READLIST_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(readlist_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .bind(width)
    .bind(height)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert readlist thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail create tx: {error}"))?;

    let record = ReadlistThumbnailRecord {
        id,
        readlist_id: readlist_id.to_string(),
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
        thumbnail: thumbnail.to_vec(),
    };
    emit_thumbnail_readlist_event(&record.readlist_id, record.selected, true);
    Ok(record)
}

pub async fn select_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist thumbnail select db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM READLIST
        WHERE ID = ?
        LIMIT 1
        "#,
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

    let target_readlist_id = sqlx::query(
        r#"
        SELECT READLIST_ID
        FROM THUMBNAIL_READLIST
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist thumbnail select target: {error}"))?
    .map(|row| row.get::<String, _>("READLIST_ID"));
    let Some(target_readlist_id) = target_readlist_id else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail select tx: {error}"))?;
        return Ok(true);
    };

    sqlx::query(
        r#"
        UPDATE THUMBNAIL_READLIST
        SET SELECTED = 0
        WHERE READLIST_ID = ?
        "#,
    )
    .bind(&target_readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected readlist thumbnails for select: {error}"))?;
    sqlx::query(
        r#"
        UPDATE THUMBNAIL_READLIST
        SET SELECTED = 1, LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected readlist thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail select tx: {error}"))?;
    emit_thumbnail_readlist_event(&target_readlist_id, true, true);
    Ok(true)
}

pub async fn delete_readlist_thumbnail(
    database_file: &Path,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist thumbnail delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM READLIST
        WHERE ID = ?
        LIMIT 1
        "#,
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

    let target = sqlx::query(
        r#"
        SELECT READLIST_ID, SELECTED
        FROM THUMBNAIL_READLIST
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query readlist thumbnail delete target: {error}"))?;
    let Some(target) = target else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist thumbnail delete tx: {error}"))?;
        return Ok(false);
    };
    let target_readlist_id = target.get::<String, _>("READLIST_ID");
    let deleted_selected = target.get::<bool, _>("SELECTED");

    sqlx::query(
        r#"
        DELETE FROM THUMBNAIL_READLIST
        WHERE ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete readlist thumbnail: {error}"))?;

    normalize_readlist_thumbnail_selection(&mut tx, &target_readlist_id, deleted_selected).await?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist thumbnail delete tx: {error}"))?;
    emit_thumbnail_readlist_event(&target_readlist_id, deleted_selected, false);
    Ok(true)
}

async fn normalize_readlist_thumbnail_selection(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    readlist_id: &str,
    deleted_selected: bool,
) -> Result<(), String> {
    let remaining_rows = sqlx::query(
        r#"
        SELECT ID, SELECTED
        FROM THUMBNAIL_READLIST
        WHERE READLIST_ID = ?
        ORDER BY ID ASC
        "#,
    )
    .bind(readlist_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| {
        format!("query remaining readlist thumbnails for delete housekeeping: {error}")
    })?;

    let selected_ids = remaining_rows
        .iter()
        .filter(|row| row.get::<bool, _>("SELECTED"))
        .map(|row| row.get::<String, _>("ID"))
        .collect::<Vec<_>>();

    let target_selected_id = if selected_ids.len() > 1 {
        selected_ids.first().cloned()
    } else if selected_ids.is_empty() && deleted_selected {
        remaining_rows.first().map(|row| row.get::<String, _>("ID"))
    } else {
        None
    };

    let Some(target_selected_id) = target_selected_id else {
        return Ok(());
    };

    sqlx::query(
        r#"
        UPDATE THUMBNAIL_READLIST
        SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END,
            LAST_MODIFIED_DATE = CASE WHEN ID = ? THEN STRFTIME('%Y-%m-%d %H:%M:%f', 'now') ELSE LAST_MODIFIED_DATE END
        WHERE READLIST_ID = ?
        "#
    )
    .bind(&target_selected_id)
    .bind(&target_selected_id)
    .bind(readlist_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("normalize readlist thumbnail selection after delete: {error}"))?;

    Ok(())
}

pub async fn load_persisted_readlist_name(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist file db: {error}"))?;
    let row = sqlx::query(
        r#"
        SELECT NAME
        FROM READLIST
        WHERE ID = ?
        "#,
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
