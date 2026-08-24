use anyhow::Context;
use komga_application::media_assets::{CollectionThumbnailRecord, ThumbnailType};
use komga_application::runtime_sse::RuntimeSseEventSink;
use sqlx::{Row, SqlitePool};

use super::{emit_thumbnail_collection_event, generated_thumbnail_id};
use crate::persistence::sqlite::codecs::parse_thumbnail_type;

pub(crate) async fn persisted_collection_exists(
    pool: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<bool> {
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
    .context("query persisted collection existence")?;
    Ok(row.is_some())
}

pub(crate) async fn load_persisted_collection_thumbnails(
    pool: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Vec<CollectionThumbnailRecord>> {
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
    .context("query persisted collection thumbnails")?;

    rows.into_iter()
        .map(|row| {
            Ok(CollectionThumbnailRecord {
                id: row.get::<String, _>("ID"),
                collection_id: row.get::<String, _>("COLLECTION_ID"),
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

pub(crate) async fn load_collection_thumbnail_by_id(
    pool: &SqlitePool,
    thumbnail_id: &str,
) -> anyhow::Result<Option<CollectionThumbnailRecord>> {
    sqlx::query(
        r#"
        SELECT ID, COLLECTION_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT, THUMBNAIL
        FROM THUMBNAIL_COLLECTION
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(pool)
    .await
    .context("query single collection thumbnail")
    .map(|row| {
        row.map(|row| CollectionThumbnailRecord {
            id: row.get::<String, _>("ID"),
            collection_id: row.get::<String, _>("COLLECTION_ID"),
            thumbnail_type: parse_thumbnail_type(&row.get::<String, _>("TYPE")),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            width: row.get::<i64, _>("WIDTH"),
            height: row.get::<i64, _>("HEIGHT"),
            thumbnail: row.get::<Vec<u8>, _>("THUMBNAIL"),
        })
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "This persistence boundary writes the thumbnail record fields directly."
)]
pub(crate) async fn insert_collection_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    collection_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> anyhow::Result<CollectionThumbnailRecord> {
    let mut tx = pool
        .begin()
        .await
        .context("begin collection thumbnail create tx")?;

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
    .context("query collection existence for thumbnail create")?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .context("rollback collection thumbnail create tx")?;
        return Err(anyhow::anyhow!("collection does not exist"));
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
        .context("clear selected collection thumbnails")?;
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
    .bind(ThumbnailType::UserUploaded.persisted_name())
    .bind(collection_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .bind(width)
    .bind(height)
    .execute(&mut *tx)
    .await
    .context("insert collection thumbnail")?;

    tx.commit()
        .await
        .context("commit collection thumbnail create tx")?;

    let record = CollectionThumbnailRecord {
        id,
        collection_id: collection_id.to_string(),
        thumbnail_type: ThumbnailType::UserUploaded,
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
        thumbnail: thumbnail.to_vec(),
    };
    emit_thumbnail_collection_event(runtime_events, &record.collection_id, record.selected, true);
    Ok(record)
}

pub(crate) async fn select_collection_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    thumbnail_id: &str,
) -> anyhow::Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("begin collection thumbnail select tx")?;

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
    .context("query target collection thumbnail for select")?
    .map(|row| row.get::<String, _>("COLLECTION_ID"));
    let Some(target_collection_id) = target_collection_id else {
        tx.rollback()
            .await
            .context("rollback collection thumbnail select tx")?;
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
    .context("clear selected collection thumbnails for select")?;
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
    .context("mark selected collection thumbnail")?;

    tx.commit()
        .await
        .context("commit collection thumbnail select tx")?;
    emit_thumbnail_collection_event(runtime_events, &target_collection_id, true, true);
    Ok(true)
}

pub(crate) async fn delete_collection_thumbnail(
    pool: &SqlitePool,
    runtime_events: &dyn RuntimeSseEventSink,
    collection_id: &str,
    thumbnail_id: &str,
) -> anyhow::Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("begin collection thumbnail delete tx")?;

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
    .context("query collection existence for thumbnail delete")?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .context("rollback collection thumbnail delete tx")?;
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
    .context("query collection thumbnail delete target")?;
    let Some(target) = target else {
        tx.rollback()
            .await
            .context("rollback collection thumbnail delete tx")?;
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
    .context("delete collection thumbnail")?;

    normalize_collection_thumbnail_selection(&mut tx, &target_collection_id, deleted_selected)
        .await?;

    tx.commit()
        .await
        .context("commit collection thumbnail delete tx")?;
    emit_thumbnail_collection_event(
        runtime_events,
        &target_collection_id,
        deleted_selected,
        false,
    );
    Ok(true)
}

async fn normalize_collection_thumbnail_selection(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    collection_id: &str,
    deleted_selected: bool,
) -> anyhow::Result<()> {
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
        anyhow::anyhow!(error)
            .context("query remaining collection thumbnails for delete housekeeping: ")
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
    .context("normalize collection thumbnail selection after delete")?;

    Ok(())
}
