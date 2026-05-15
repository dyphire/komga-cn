use komga_application::media_assets::{EntityThumbnailBinary, EntityThumbnailRecord};
use sqlx::{Row, SqlitePool};

use super::{emit_thumbnail_book_event, generated_thumbnail_id};

pub async fn load_persisted_book_thumbnails(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Vec<EntityThumbnailRecord>, String> {
    let rows = sqlx::query(
        r#"
        SELECT ID, BOOK_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT
        FROM THUMBNAIL_BOOK
        WHERE BOOK_ID = ?
        "#,
    )
    .bind(book_id)
    .fetch_all(pool)
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
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    let row = sqlx::query(
        r#"
        SELECT TYPE, MEDIA_TYPE, THUMBNAIL
        FROM THUMBNAIL_BOOK
        WHERE BOOK_ID = ?
        ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query selected book thumbnail: {error}"))?;

    Ok(row.map(|row| EntityThumbnailBinary {
        thumbnail_type: row.get::<String, _>("TYPE"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

pub async fn load_book_thumbnail_by_id(
    pool: &SqlitePool,
    thumbnail_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    let row = sqlx::query(
        r#"
        SELECT TYPE, MEDIA_TYPE, THUMBNAIL
        FROM THUMBNAIL_BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query single book thumbnail: {error}"))?;

    Ok(row.map(|row| EntityThumbnailBinary {
        thumbnail_type: row.get::<String, _>("TYPE"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
    }))
}

async fn load_book_series_id(pool: &SqlitePool, book_id: &str) -> Result<Option<String>, String> {
    sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("query book series id for thumbnail SSE: {error}"))
        .map(|row| row.map(|row| row.get::<String, _>("SERIES_ID")))
}

pub async fn insert_book_thumbnail(
    pool: &SqlitePool,
    book_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<EntityThumbnailRecord, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
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
            r#"
            UPDATE THUMBNAIL_BOOK
            SET SELECTED = 0
            WHERE BOOK_ID = ?
            "#,
        )
        .bind(book_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected book thumbnails: {error}"))?;
    }

    let selected = if selected {
        true
    } else {
        sqlx::query(
            r#"
            SELECT 1 AS FOUND
            FROM THUMBNAIL_BOOK
            WHERE BOOK_ID = ? AND SELECTED = 1
            LIMIT 1
            "#,
        )
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query selected book thumbnails for housekeeping: {error}"))?
        .is_none()
    };

    let id = generated_thumbnail_id("thumbnail-book");
    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_BOOK
        (ID, SELECTED, THUMBNAIL, TYPE, BOOK_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
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

    let series_id = load_book_series_id(pool, book_id)
        .await?
        .unwrap_or_default();
    let record = EntityThumbnailRecord {
        id,
        book_id: book_id.to_string(),
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
    };
    emit_thumbnail_book_event(&record.book_id, &series_id, record.selected, true);
    Ok(record)
}

pub async fn select_book_thumbnail(pool: &SqlitePool, thumbnail_id: &str) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail select tx: {error}"))?;

    let target_book_id = sqlx::query(
        r#"
        SELECT BOOK_ID
        FROM THUMBNAIL_BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
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
        r#"
        UPDATE THUMBNAIL_BOOK
        SET SELECTED = 0
        WHERE BOOK_ID = ?
        "#,
    )
    .bind(&target_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected book thumbnails for select: {error}"))?;
    sqlx::query(
        r#"
        UPDATE THUMBNAIL_BOOK
        SET SELECTED = 1, LAST_MODIFIED_DATE = STRFTIME('%Y-%m-%d %H:%M:%f', 'now')
        WHERE ID = ? AND BOOK_ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .bind(&target_book_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected book thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail select tx: {error}"))?;
    let selected_series_id = load_book_series_id(pool, &target_book_id)
        .await?
        .unwrap_or_default();
    emit_thumbnail_book_event(&target_book_id, &selected_series_id, true, true);
    Ok(true)
}

pub async fn delete_book_thumbnail(pool: &SqlitePool, thumbnail_id: &str) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin book thumbnail delete tx: {error}"))?;

    let target = sqlx::query(
        r#"
        SELECT BOOK_ID, TYPE, SELECTED
        FROM THUMBNAIL_BOOK
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query book thumbnail delete target: {error}"))?;
    let Some(target) = target else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback book thumbnail delete tx: {error}"))?;
        return Ok(false);
    };
    let target_book_id = target.get::<String, _>("BOOK_ID");
    let target_type = target.get::<String, _>("TYPE");
    let deleted_selected = target.get::<bool, _>("SELECTED");
    let series_id = sqlx::query("SELECT SERIES_ID FROM BOOK WHERE ID = ? LIMIT 1")
        .bind(&target_book_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|error| format!("query book series id for thumbnail delete: {error}"))?
        .map(|row| row.get::<String, _>("SERIES_ID"))
        .unwrap_or_default();
    if target_type != "USER_UPLOADED" {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback generated book thumbnail delete tx: {error}"))?;
        return Err("only uploaded thumbnails can be deleted".to_string());
    }

    sqlx::query(
        r#"
        DELETE FROM THUMBNAIL_BOOK
        WHERE ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete book thumbnail: {error}"))?;

    normalize_book_thumbnail_selection(&mut tx, &target_book_id, deleted_selected).await?;

    tx.commit()
        .await
        .map_err(|error| format!("commit book thumbnail delete tx: {error}"))?;
    emit_thumbnail_book_event(&target_book_id, &series_id, deleted_selected, false);
    Ok(true)
}

async fn normalize_book_thumbnail_selection(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    book_id: &str,
    deleted_selected: bool,
) -> Result<(), String> {
    let remaining_rows = sqlx::query(
        r#"
        SELECT ID, SELECTED
        FROM THUMBNAIL_BOOK
        WHERE BOOK_ID = ?
        ORDER BY ID ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("query remaining book thumbnails for delete housekeeping: {error}"))?;

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
        UPDATE THUMBNAIL_BOOK
        SET SELECTED = CASE WHEN ID = ? THEN 1 ELSE 0 END,
            LAST_MODIFIED_DATE = CASE WHEN ID = ? THEN STRFTIME('%Y-%m-%d %H:%M:%f', 'now') ELSE LAST_MODIFIED_DATE END
        WHERE BOOK_ID = ?
        "#,
    )
    .bind(&target_selected_id)
    .bind(&target_selected_id)
    .bind(book_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("normalize book thumbnail selection after delete: {error}"))?;

    Ok(())
}
