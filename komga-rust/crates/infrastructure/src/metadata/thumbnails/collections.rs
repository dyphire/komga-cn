use komga_application::media_assets::CollectionThumbnailRecord;
use sqlx::{Row, SqlitePool};

use super::{emit_thumbnail_collection_event, generated_thumbnail_id};

pub async fn persisted_collection_exists(
    pool: &SqlitePool,
    collection_id: &str,
) -> Result<bool, String> {
    let row = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM COLLECTION
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(collection_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted collection existence: {error}"))?;
    Ok(row.is_some())
}

pub async fn load_persisted_collection_thumbnails(
    pool: &SqlitePool,
    collection_id: &str,
) -> Result<Vec<CollectionThumbnailRecord>, String> {
    let rows = sqlx::query(
        r#"
        SELECT ID, COLLECTION_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, THUMBNAIL
        FROM THUMBNAIL_COLLECTION
        WHERE COLLECTION_ID = ?
        ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC
        "#,
    )
    .bind(collection_id)
    .fetch_all(pool)
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
    pool: &SqlitePool,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<CollectionThumbnailRecord, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM COLLECTION
        WHERE ID = ?
        LIMIT 1
        "#,
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
            r#"
            UPDATE THUMBNAIL_COLLECTION
            SET SELECTED = 0
            WHERE COLLECTION_ID = ?
            "#,
        )
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected collection thumbnails: {error}"))?;
    }

    let id = generated_thumbnail_id("thumbnail-collection");
    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_COLLECTION
            (ID, SELECTED, THUMBNAIL, TYPE, COLLECTION_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
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

    let record = CollectionThumbnailRecord {
        id,
        collection_id: collection_id.to_string(),
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
        thumbnail: thumbnail.to_vec(),
    };
    emit_thumbnail_collection_event(&record.collection_id, record.selected, true);
    Ok(record)
}

pub async fn select_collection_thumbnail(
    pool: &SqlitePool,
    thumbnail_id: &str,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail select tx: {error}"))?;

    let target_collection_id = sqlx::query(
        r#"
        SELECT COLLECTION_ID
        FROM THUMBNAIL_COLLECTION
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target collection thumbnail for select: {error}"))?
    .map(|row| row.get::<String, _>("COLLECTION_ID"));
    let Some(target_collection_id) = target_collection_id else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail select tx: {error}"))?;
        return Ok(false);
    };

    sqlx::query(
        r#"
        UPDATE THUMBNAIL_COLLECTION
        SET SELECTED = 0
        WHERE COLLECTION_ID = ?
        "#,
    )
    .bind(&target_collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected collection thumbnails for select: {error}"))?;
    sqlx::query(
        r#"
        UPDATE THUMBNAIL_COLLECTION
        SET SELECTED = 1, LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ? AND COLLECTION_ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .bind(&target_collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected collection thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail select tx: {error}"))?;
    emit_thumbnail_collection_event(&target_collection_id, true, true);
    Ok(true)
}

pub async fn delete_collection_thumbnail(
    pool: &SqlitePool,
    collection_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM COLLECTION
        WHERE ID = ?
        LIMIT 1
        "#,
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

    let target = sqlx::query(
        r#"
        SELECT COLLECTION_ID, SELECTED
        FROM THUMBNAIL_COLLECTION
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query collection thumbnail delete target: {error}"))?;
    let Some(target) = target else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection thumbnail delete tx: {error}"))?;
        return Ok(false);
    };
    let target_collection_id = target.get::<String, _>("COLLECTION_ID");
    let deleted_selected = target.get::<bool, _>("SELECTED");

    sqlx::query(
        r#"
        DELETE FROM THUMBNAIL_COLLECTION
        WHERE ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete collection thumbnail: {error}"))?;

    normalize_collection_thumbnail_selection(&mut tx, &target_collection_id, deleted_selected)
        .await?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection thumbnail delete tx: {error}"))?;
    emit_thumbnail_collection_event(&target_collection_id, deleted_selected, false);
    Ok(true)
}

async fn normalize_collection_thumbnail_selection(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    collection_id: &str,
    deleted_selected: bool,
) -> Result<(), String> {
    let remaining_rows = sqlx::query(
        r#"
        SELECT ID, SELECTED
        FROM THUMBNAIL_COLLECTION
        WHERE COLLECTION_ID = ?
        ORDER BY ID ASC
        "#,
    )
    .bind(collection_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| {
        format!("query remaining collection thumbnails for delete housekeeping: {error}")
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
        UPDATE THUMBNAIL_COLLECTION
        SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END,
            LAST_MODIFIED_DATE = CASE WHEN ID = ? THEN STRFTIME('%Y-%m-%d %H:%M:%f', 'now') ELSE LAST_MODIFIED_DATE END
        WHERE COLLECTION_ID = ?
        "#
    )
    .bind(&target_selected_id)
    .bind(&target_selected_id)
    .bind(collection_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("normalize collection thumbnail selection after delete: {error}"))?;

    Ok(())
}
