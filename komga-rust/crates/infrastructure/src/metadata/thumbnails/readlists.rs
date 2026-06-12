use komga_application::media_assets::{ReadlistThumbnailRecord, ThumbnailType};
use komga_application::runtime_sse::RuntimeSseEventSink;
use sqlx::{Row, SqlitePool};

use super::{emit_thumbnail_readlist_event, generated_thumbnail_id};
use crate::parsing::parse_thumbnail_type;

pub(crate) async fn load_persisted_readlist_thumbnails(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Vec<ReadlistThumbnailRecord>, String> {
    let rows = sqlx::query(
        r#"
        SELECT ID, READLIST_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, THUMBNAIL
        FROM THUMBNAIL_READLIST
        WHERE READLIST_ID = ?
        ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC
        "#,
    )
    .bind(readlist_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted readlist thumbnails: {error}"))?;

    rows.into_iter()
        .map(|row| {
            Ok(ReadlistThumbnailRecord {
                id: row.get::<String, _>("ID"),
                readlist_id: row.get::<String, _>("READLIST_ID"),
                thumbnail_type: parse_thumbnail_type(&row.get::<String, _>("TYPE")),
                selected: row.get::<i64, _>("SELECTED") != 0,
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                file_size: row.get::<i64, _>("FILE_SIZE"),
                width: row.get::<i64, _>("WIDTH"),
                height: row.get::<i64, _>("HEIGHT"),
                thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
            })
        })
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "This persistence boundary writes the thumbnail record fields directly."
)]
pub(crate) async fn insert_readlist_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    readlist_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<ReadlistThumbnailRecord, String> {
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
    .bind(ThumbnailType::UserUploaded.persisted_name())
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
        thumbnail_type: ThumbnailType::UserUploaded,
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
        thumbnail: thumbnail.to_vec(),
    };
    emit_thumbnail_readlist_event(runtime_events, &record.readlist_id, record.selected, true);
    Ok(record)
}

pub(crate) async fn select_readlist_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
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
    emit_thumbnail_readlist_event(runtime_events, &target_readlist_id, true, true);
    Ok(true)
}

pub(crate) async fn delete_readlist_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    readlist_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
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
    emit_thumbnail_readlist_event(runtime_events, &target_readlist_id, deleted_selected, false);
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

pub(crate) async fn load_persisted_readlist_name(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Option<String>, String> {
    let row = sqlx::query(
        r#"
        SELECT NAME
        FROM READLIST
        WHERE ID = ?
        "#,
    )
    .bind(readlist_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted readlist name: {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("NAME")))
}

pub(crate) async fn persisted_readlist_exists(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<bool, String> {
    Ok(load_persisted_readlist_name(pool, readlist_id)
        .await?
        .is_some())
}
