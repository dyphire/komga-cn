use std::path::Path as FsPath;

use crate::sqlite::connect_pool;
use sqlx::Row;

#[derive(Clone)]
pub struct PersistedCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct PersistedSeriesRestrictionRecord {
    pub age_rating: Option<u16>,
    pub labels: Vec<String>,
}

pub async fn persisted_collections_exist(database_file: &FsPath) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collections exists db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT 1 AS FOUND
FROM COLLECTION
LIMIT 1"#,
    )
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted collections existence: {error}"))?;
    Ok(row.is_some())
}

pub async fn load_persisted_collections(
    database_file: &FsPath,
) -> Result<Vec<PersistedCollectionRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted collections db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM COLLECTION
ORDER BY NAME COLLATE NOCASE ASC"#,
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted collections: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedCollectionRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            ordered: row.get::<bool, _>("ORDERED"),
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
        })
        .collect())
}

pub async fn load_persisted_collection_detail(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<PersistedCollectionRecord>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted collection detail db: {error}"))?;

    let row = sqlx::query(
        r#"SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM COLLECTION
WHERE ID = ?"#,
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted collection detail: {error}"))?;

    Ok(row.map(|row| PersistedCollectionRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        ordered: row.get::<bool, _>("ORDERED"),
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
    }))
}

pub async fn load_persisted_collection_series_ids(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted collection series ids db: {error}"))?;

    let rows = sqlx::query(
        r#"SELECT SERIES_ID
FROM COLLECTION_SERIES
WHERE COLLECTION_ID = ?
ORDER BY NUMBER ASC"#,
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted collection series ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("SERIES_ID"))
        .collect())
}

pub async fn load_series_library_id(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series visibility db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT LIBRARY_ID
FROM SERIES
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series library for visibility: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

pub async fn load_series_restrictions(
    database_file: &FsPath,
    series_id: &str,
) -> Result<PersistedSeriesRestrictionRecord, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series restrictions db: {error}"))?;

    let age_row = sqlx::query(
        r#"SELECT AGE_RATING
FROM SERIES_METADATA
WHERE SERIES_ID = ?
LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series age rating for visibility: {error}"))?;

    let label_rows = sqlx::query(
        r#"SELECT LABEL
FROM SERIES_METADATA_SHARING
WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series sharing labels for visibility: {error}"))?;

    let age_rating = age_row
        .and_then(|row| row.get::<Option<i64>, _>("AGE_RATING"))
        .and_then(|value| u16::try_from(value).ok());
    let labels = label_rows
        .into_iter()
        .map(|row| row.get::<String, _>("LABEL"))
        .collect::<Vec<_>>();

    Ok(PersistedSeriesRestrictionRecord { age_rating, labels })
}

pub async fn persist_collection_create(
    database_file: &FsPath,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection create db: {error}"))?;
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

    replace_collection_series(&mut tx, collection_id, series_ids)
        .await
        .map_err(|error| format!("insert persisted collection series: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection create tx: {error}"))?;

    Ok(())
}

pub async fn persist_collection_update(
    database_file: &FsPath,
    collection_id: &str,
    name: &str,
    ordered: bool,
    series_ids: &[String],
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection update db: {error}"))?;
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

    replace_collection_series(&mut tx, collection_id, series_ids)
        .await
        .map_err(|error| format!("replace persisted collection series: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection update tx: {error}"))?;
    Ok(true)
}

pub async fn delete_persisted_collection(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection delete tx: {error}"))?;

    sqlx::query(
        r#"DELETE
FROM THUMBNAIL_COLLECTION
WHERE COLLECTION_ID = ?"#,
    )
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted collection thumbnails: {error}"))?;

    sqlx::query(
        r#"DELETE
FROM COLLECTION_SERIES
WHERE COLLECTION_ID = ?"#,
    )
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted collection series: {error}"))?;

    let deleted = sqlx::query(
        r#"DELETE
FROM COLLECTION
WHERE ID = ?"#,
    )
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted collection: {error}"))?
    .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit collection delete tx: {error}"))?;
    Ok(true)
}

async fn replace_collection_series(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    collection_id: &str,
    series_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"DELETE
FROM COLLECTION_SERIES
WHERE COLLECTION_ID = ?"#,
    )
    .bind(collection_id)
    .execute(&mut **tx)
    .await?;

    for (index, series_id) in series_ids.iter().enumerate() {
        sqlx::query(
            r#"INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER)
VALUES (?, ?, ?)"#,
        )
        .bind(collection_id)
        .bind(series_id)
        .bind(index as i64)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
