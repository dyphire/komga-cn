use std::path::Path as FsPath;

use crate::sqlite::connect_pool;
use sqlx::Row;

#[derive(Clone)]
pub struct PersistedSeriesResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedSeriesDetailRecord {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub title_sort: String,
    pub url: String,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub books_count: u32,
    pub status: String,
    pub summary: String,
    pub reading_direction: String,
    pub publisher: String,
    pub age_rating: Option<u16>,
    pub language: String,
    pub sharing_labels: String,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct ExistingSeriesMetadataRecord {
    pub title: String,
    pub title_sort: String,
    pub summary: String,
}

pub async fn load_persisted_series_resource(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedSeriesResourceRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.LIBRARY_ID, sm.AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS \
         FROM SERIES s \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE s.ID = ? \
         GROUP BY s.LIBRARY_ID, sm.AGE_RATING",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series resource: {error}"))?;

    Ok(row.map(|row| PersistedSeriesResourceRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value as u16),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
    }))
}

pub async fn load_series_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series-id remap db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS ID \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.DELETED_DATE IS NULL \
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC \
         LIMIT 1 \
         OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped series id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("ID")))
}

pub async fn load_persisted_series_detail(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedSeriesDetailRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS ID, s.LIBRARY_ID AS LIBRARY_ID, s.URL AS URL, \
                s.CREATED_DATE AS CREATED_DATE, s.LAST_MODIFIED_DATE AS LAST_MODIFIED_DATE, \
                CAST(s.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED, s.ONESHOT AS ONESHOT, \
                s.DELETED_DATE AS DELETED_DATE, sm.STATUS AS STATUS, sm.TITLE AS TITLE, \
                sm.TITLE_SORT AS TITLE_SORT, \
                sm.SUMMARY AS SUMMARY, sm.READING_DIRECTION AS READING_DIRECTION, \
                sm.PUBLISHER AS PUBLISHER, sm.AGE_RATING AS AGE_RATING, sm.LANGUAGE AS LANGUAGE, \
                sm.CREATED_DATE AS METADATA_CREATED, \
                sm.LAST_MODIFIED_DATE AS METADATA_LAST_MODIFIED, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS \
         FROM SERIES s \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE s.ID = ? \
         GROUP BY s.ID, s.LIBRARY_ID, s.URL, s.CREATED_DATE, s.LAST_MODIFIED_DATE, \
                  s.FILE_LAST_MODIFIED, s.ONESHOT, s.DELETED_DATE, sm.STATUS, sm.TITLE, \
                  sm.SUMMARY, sm.READING_DIRECTION, sm.PUBLISHER, sm.AGE_RATING, sm.LANGUAGE, \
                  METADATA_CREATED, METADATA_LAST_MODIFIED",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series detail: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let books_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) \
         FROM BOOK \
         WHERE SERIES_ID = ?",
    )
    .bind(series_id)
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("query persisted series books count: {error}"))?;

    Ok(Some(PersistedSeriesDetailRecord {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        url: row.get::<String, _>("URL"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        books_count: books_count as u32,
        status: row.get::<String, _>("STATUS"),
        summary: row.get::<String, _>("SUMMARY"),
        reading_direction: row
            .get::<Option<String>, _>("READING_DIRECTION")
            .unwrap_or_default(),
        publisher: row.get::<String, _>("PUBLISHER"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value as u16),
        language: row.get::<String, _>("LANGUAGE"),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<bool, _>("ONESHOT"),
    }))
}

pub async fn load_persisted_series_collections(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<PersistedCollectionRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series collection db: {error}"))?;

    let rows = sqlx::query(
        "SELECT c.ID, c.NAME, c.ORDERED, c.CREATED_DATE, c.LAST_MODIFIED_DATE \
         FROM COLLECTION c \
         JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID \
         WHERE cs.SERIES_ID = ? \
         ORDER BY c.NAME COLLATE NOCASE ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted series collections: {error}"))?;

    let mut collections = Vec::with_capacity(rows.len());
    for row in rows {
        let collection_id = row.get::<String, _>("ID");
        let series_ids_rows = sqlx::query(
            "SELECT SERIES_ID \
             FROM COLLECTION_SERIES \
             WHERE COLLECTION_ID = ? \
             ORDER BY NUMBER ASC",
        )
        .bind(collection_id.clone())
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted collection series ids: {error}"))?;

        collections.push(PersistedCollectionRecord {
            id: collection_id,
            name: row.get::<String, _>("NAME"),
            ordered: row.get::<bool, _>("ORDERED"),
            series_ids: series_ids_rows
                .into_iter()
                .map(|series_row| series_row.get::<String, _>("SERIES_ID"))
                .collect(),
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
        });
    }

    Ok(collections)
}

pub async fn load_existing_series_metadata(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata db: {error}"))?;

    let row = sqlx::query(
        "SELECT TITLE, TITLE_SORT, SUMMARY \
         FROM SERIES_METADATA \
         WHERE SERIES_ID = ?",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query existing series metadata: {error}"))?;

    Ok(row.map(|row| ExistingSeriesMetadataRecord {
        title: row.get::<String, _>("TITLE"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        summary: row.get::<String, _>("SUMMARY"),
    }))
}

pub async fn persist_series_metadata_update(
    database_file: &FsPath,
    series_id: &str,
    title: &str,
    title_sort: &str,
    summary: &str,
) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata update db: {error}"))?;

    let result = sqlx::query(
        "UPDATE SERIES_METADATA \
         SET TITLE = ?, TITLE_SORT = ?, SUMMARY = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE SERIES_ID = ?",
    )
    .bind(title)
    .bind(title_sort)
    .bind(summary)
    .bind(series_id)
    .execute(&pool)
    .await
    .map_err(|error| format!("persist series metadata update: {error}"))?;

    Ok(result.rows_affected() > 0)
}

pub async fn refresh_series_after_metadata_update(
    database_file: &FsPath,
    series_id: &str,
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata refresh db: {error}"))?;

    sqlx::query(
        "UPDATE SERIES_METADATA \
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE SERIES_ID = ?",
    )
    .bind(series_id)
    .execute(&pool)
    .await
    .map_err(|error| format!("refresh series metadata timestamp: {error}"))?;

    sqlx::query(
        "UPDATE SERIES \
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE ID = ?",
    )
    .bind(series_id)
    .execute(&pool)
    .await
    .map_err(|error| format!("refresh series row timestamp: {error}"))?;

    Ok(())
}
