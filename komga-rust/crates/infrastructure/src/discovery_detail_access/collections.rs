use sqlx::{Row, SqlitePool};

use super::common;

use komga_application::discovery::{
    PersistedCollectionAccessRecord, PersistedSeriesRestrictionRecord,
};

pub(super) async fn persisted_collections_exist(pool: &SqlitePool) -> Result<bool, String> {
    common::table_has_rows(pool, "COLLECTION", "persisted collections").await
}

pub(super) async fn load_persisted_collections(
    pool: &SqlitePool,
) -> Result<Vec<PersistedCollectionAccessRecord>, String> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM COLLECTION
ORDER BY NAME COLLATE NOCASE ASC"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted collections: {error}"))?;

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
) -> Result<Option<PersistedCollectionAccessRecord>, String> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM COLLECTION
WHERE ID = ?"#,
    )
    .bind(collection_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted collection detail: {error}"))?;

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
) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID
FROM COLLECTION_SERIES
WHERE COLLECTION_ID = ?
ORDER BY NUMBER ASC"#,
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted collection series ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("SERIES_ID"))
        .collect())
}

pub(super) async fn load_series_library_id(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<String>, String> {
    let row = sqlx::query(
        r#"SELECT LIBRARY_ID
FROM SERIES
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query series library for visibility: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

pub(super) async fn load_series_restrictions(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<PersistedSeriesRestrictionRecord, String> {
    let age_row = sqlx::query(
        r#"SELECT AGE_RATING
FROM SERIES_METADATA
WHERE SERIES_ID = ?
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query series age rating for visibility: {error}"))?;

    let label_rows = sqlx::query(
        r#"SELECT LABEL
FROM SERIES_METADATA_SHARING
WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query series sharing labels for visibility: {error}"))?;

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
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection create tx: {error}"))?;

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
    .map_err(|error| format!("insert persisted collection: {error}"))?;

    common::replace_ordered_children(
        &mut tx,
        "COLLECTION_SERIES",
        "COLLECTION_ID",
        "SERIES_ID",
        collection_id,
        series_ids,
    )
    .await
    .map_err(|error| format!("insert persisted collection series: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection create tx: {error}"))?;

    Ok(())
}

pub(super) async fn persist_collection_update(
    pool: &SqlitePool,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection update tx: {error}"))?;

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
    .map_err(|error| format!("update persisted collection: {error}"))?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection update tx: {error}"))?;
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
    .map_err(|error| format!("replace persisted collection series: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection update tx: {error}"))?;
    Ok(true)
}

pub(super) async fn delete_persisted_collection(
    pool: &SqlitePool,
    collection_id: &str,
) -> Result<bool, String> {
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
