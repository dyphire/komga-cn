use komga_application::media_assets::{EntityThumbnailBinary, SeriesThumbnailRecord};
use reqwest::Url;
use sqlx::{Row, SqlitePool};

use super::{emit_thumbnail_series_event, generated_thumbnail_id};

pub async fn load_persisted_series_thumbnails(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Vec<SeriesThumbnailRecord>, String> {
    let rows = sqlx::query(
        r#"
        SELECT ID, SERIES_ID, TYPE, SELECTED, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT
        FROM THUMBNAIL_SERIES
        WHERE SERIES_ID = ?
        ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC
        "#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted series thumbnails: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| SeriesThumbnailRecord {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            width: row.get::<i64, _>("WIDTH"),
            height: row.get::<i64, _>("HEIGHT"),
        })
        .collect())
}

pub async fn load_selected_series_thumbnail(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    let row = sqlx::query(
        r#"
        SELECT TYPE, MEDIA_TYPE, THUMBNAIL
        FROM THUMBNAIL_SERIES
        WHERE SERIES_ID = ?
        ORDER BY SELECTED DESC, LAST_MODIFIED_DATE DESC, ID ASC
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query selected series thumbnail: {error}"))?;

    Ok(row.map(|row| EntityThumbnailBinary {
        thumbnail_type: row.get::<String, _>("TYPE"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        thumbnail: row
            .get::<Option<Vec<u8>>, _>("THUMBNAIL")
            .unwrap_or_default(),
    }))
}

pub async fn load_series_thumbnail_by_id(
    pool: &SqlitePool,
    thumbnail_id: &str,
) -> Result<Option<EntityThumbnailBinary>, String> {
    let row = sqlx::query(
        r#"
        SELECT TYPE, MEDIA_TYPE, THUMBNAIL, URL
        FROM THUMBNAIL_SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query single series thumbnail: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let thumbnail_type = row.get::<String, _>("TYPE");
    let media_type = row.get::<String, _>("MEDIA_TYPE");
    let thumbnail = row.get::<Option<Vec<u8>>, _>("THUMBNAIL");
    let url = row.get::<Option<String>, _>("URL");
    let maybe_thumbnail = load_thumbnail_bytes_or_sidecar(thumbnail, url)?;

    Ok(maybe_thumbnail.map(|thumbnail| EntityThumbnailBinary {
        thumbnail_type,
        media_type,
        thumbnail,
    }))
}

pub async fn insert_series_thumbnail(
    pool: &SqlitePool,
    series_id: &str,
    thumbnail: &[u8],
    media_type: &str,
    width: i64,
    height: i64,
    selected: bool,
) -> Result<SeriesThumbnailRecord, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series thumbnail create tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series existence for thumbnail create: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail create tx: {error}"))?;
        return Err("series does not exist".to_string());
    }

    if selected {
        sqlx::query(
            r#"
            UPDATE THUMBNAIL_SERIES
            SET SELECTED = 0
            WHERE SERIES_ID = ?
            "#,
        )
        .bind(series_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("clear selected series thumbnails: {error}"))?;
    }

    let id = generated_thumbnail_id("thumbnail-series");
    sqlx::query(
        r#"
        INSERT INTO THUMBNAIL_SERIES
            (ID, SELECTED, THUMBNAIL, TYPE, SERIES_ID, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(selected)
    .bind(thumbnail)
    .bind("USER_UPLOADED")
    .bind(series_id)
    .bind(media_type)
    .bind(thumbnail.len() as i64)
    .bind(width)
    .bind(height)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert series thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit series thumbnail create tx: {error}"))?;

    let record = SeriesThumbnailRecord {
        id,
        series_id: series_id.to_string(),
        thumbnail_type: "USER_UPLOADED".to_string(),
        selected,
        media_type: media_type.to_string(),
        file_size: thumbnail.len() as i64,
        width,
        height,
    };
    emit_thumbnail_series_event(&record.series_id, record.selected, true);
    Ok(record)
}

fn load_thumbnail_bytes_or_sidecar(
    thumbnail: Option<Vec<u8>>,
    url: Option<String>,
) -> Result<Option<Vec<u8>>, String> {
    if let Some(thumbnail) = thumbnail {
        return Ok(Some(thumbnail));
    }

    let Some(url) = url else {
        return Ok(None);
    };

    let parsed =
        Url::parse(&url).map_err(|error| format!("parse series thumbnail sidecar url: {error}"))?;
    let path = parsed
        .to_file_path()
        .map_err(|_| format!("series thumbnail sidecar url is not a file path: {url}"))?;
    let bytes = std::fs::read(&path)
        .map_err(|error| format!("read series thumbnail sidecar {}: {error}", path.display()))?;
    Ok(Some(bytes))
}

pub async fn select_series_thumbnail(
    pool: &SqlitePool,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series thumbnail select tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series existence for thumbnail select: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail select tx: {error}"))?;
        return Ok(false);
    }

    let target_series_id = sqlx::query(
        r#"
        SELECT SERIES_ID
        FROM THUMBNAIL_SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query target series thumbnail for select: {error}"))?
    .map(|row| row.get::<String, _>("SERIES_ID"));
    let Some(target_series_id) = target_series_id else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail select tx: {error}"))?;
        return Ok(false);
    };

    sqlx::query(
        r#"
        UPDATE THUMBNAIL_SERIES
        SET SELECTED = 0
        WHERE SERIES_ID = ?
        "#,
    )
    .bind(&target_series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("clear selected series thumbnails for select: {error}"))?;
    sqlx::query(
        r#"
        UPDATE THUMBNAIL_SERIES
        SET SELECTED = 1
        WHERE ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("mark selected series thumbnail: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit series thumbnail select tx: {error}"))?;
    emit_thumbnail_series_event(&target_series_id, true, true);
    Ok(true)
}

pub async fn delete_series_thumbnail(
    pool: &SqlitePool,
    series_id: &str,
    thumbnail_id: &str,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series thumbnail delete tx: {error}"))?;

    let exists = sqlx::query(
        r#"
        SELECT 1 AS FOUND
        FROM SERIES
        WHERE ID = ?
        LIMIT 1
        "#,
    )
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series existence for thumbnail delete: {error}"))?
    .is_some();
    if !exists {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    let target = sqlx::query(
        r#"
        SELECT SELECTED
        FROM THUMBNAIL_SERIES
        WHERE ID = ? AND SERIES_ID = ?
        LIMIT 1
        "#,
    )
    .bind(thumbnail_id)
    .bind(series_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|error| format!("query series thumbnail delete target: {error}"))?;
    let Some(target) = target else {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail delete tx: {error}"))?;
        return Ok(false);
    };
    let deleted_selected = target.get::<bool, _>("SELECTED");

    let deleted = sqlx::query(
        r#"
        DELETE FROM THUMBNAIL_SERIES
        WHERE ID = ? AND SERIES_ID = ?
        "#,
    )
    .bind(thumbnail_id)
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete series thumbnail: {error}"))?
    .rows_affected()
        > 0;
    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series thumbnail delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit series thumbnail delete tx: {error}"))?;
    emit_thumbnail_series_event(series_id, deleted_selected, false);
    Ok(true)
}
