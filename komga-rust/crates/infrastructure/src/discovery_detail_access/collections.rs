use anyhow::Context;
use sqlx::{Row, SqlitePool};

use super::common;

use komga_application::discovery::{
    PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord,
};

pub(super) async fn persisted_collections_exist(pool: &SqlitePool) -> anyhow::Result<bool> {
    common::table_has_rows(pool, "COLLECTION", "persisted collections").await
}

pub(super) async fn load_persisted_collections(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<PersistedCollectionAccessRecord>> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM COLLECTION
ORDER BY NAME COLLATE NOCASE ASC"#,
    )
    .fetch_all(pool)
    .await
    .context("query persisted collections")?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedCollectionAccessRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            ordered: row.get::<bool, _>("ORDERED"),
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
        })
        .collect())
}

pub(super) async fn load_persisted_collection_detail(
    pool: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Option<PersistedCollectionAccessRecord>> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM COLLECTION
WHERE ID = ?"#,
    )
    .bind(collection_id)
    .fetch_optional(pool)
    .await
    .context("query persisted collection detail")?;

    Ok(row.map(|row| PersistedCollectionAccessRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        ordered: row.get::<bool, _>("ORDERED"),
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
    }))
}

pub(super) async fn load_persisted_collection_series_ids(
    pool: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID
FROM COLLECTION_SERIES
WHERE COLLECTION_ID = ?
ORDER BY NUMBER ASC"#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .context("query persisted collection series ids")?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("SERIES_ID"))
        .collect())
}

pub(super) async fn load_series_library_id(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<String>> {
    let row = sqlx::query(
        r#"SELECT LIBRARY_ID
FROM SERIES
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .context("query series library for visibility")?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

pub(super) async fn load_series_restrictions(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<PersistedSeriesRestrictionRecord> {
    let age_row = sqlx::query(
        r#"SELECT AGE_RATING
FROM SERIES_METADATA
WHERE SERIES_ID = ?
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .context("query series age rating for visibility")?;

    let label_rows = sqlx::query(
        r#"SELECT LABEL
FROM SERIES_METADATA_SHARING
WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query series sharing labels for visibility")?;

    let age_rating = age_row
        .and_then(|row| row.get::<Option<i64>, _>("AGE_RATING"))
        .map(common::clamp_kotlin_int_u32);
    let labels = label_rows
        .into_iter()
        .map(|row| row.get::<String, _>("LABEL"))
        .collect::<Vec<_>>();

    Ok(PersistedSeriesRestrictionRecord { age_rating, labels })
}

pub(super) async fn persist_collection_create(
    pool: &SqlitePool,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await.context("begin collection create tx")?;

    sqlx::query(
        r#"INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE,
LAST_MODIFIED_DATE)
VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)"#,
    )
    .bind(collection_id)
    .bind(name)
    .bind(ordered)
    .bind(series_ids.len() as i64)
    .execute(&mut *tx)
    .await
    .context("insert persisted collection")?;

    common::replace_ordered_children(
        &mut tx,
        "COLLECTION_SERIES",
        "COLLECTION_ID",
        "SERIES_ID",
        collection_id,
        series_ids,
    )
    .await
    .context("insert persisted collection series")?;

    tx.commit().await.context("commit collection create tx")?;

    Ok(())
}

pub(super) async fn persist_collection_update(
    pool: &SqlitePool,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await.context("begin collection update tx")?;

    let updated = sqlx::query(
        r#"UPDATE COLLECTION
SET NAME = ?, ORDERED = ?, SERIES_COUNT = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
    )
    .bind(name)
    .bind(ordered)
    .bind(series_ids.len() as i64)
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .context("update persisted collection")?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback()
            .await
            .context("rollback collection update tx")?;
        return Ok(false);
    }

    common::replace_ordered_children(
        &mut tx,
        "COLLECTION_SERIES",
        "COLLECTION_ID",
        "SERIES_ID",
        collection_id,
        series_ids,
    )
    .await
    .context("replace persisted collection series")?;

    tx.commit().await.context("commit collection update tx")?;
    Ok(true)
}

pub(super) async fn delete_persisted_collection(
    pool: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<bool> {
    common::delete_parent_with_children(
        pool,
        "THUMBNAIL_COLLECTION",
        "COLLECTION_SERIES",
        "COLLECTION",
        "COLLECTION_ID",
        collection_id,
        "collection",
    )
    .await
}
