use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct PersistedSeriesResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u32>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedSeriesDetailRecord {
    pub id: String,
    pub library_id: String,
    pub name: String,
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
    pub age_rating: Option<u32>,
    pub language: String,
    pub sharing_labels: String,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub deleted: bool,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedSeriesCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<String>,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct ExistingSeriesMetadataRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

#[derive(Clone)]
pub struct SeriesMetadataLinkRecord {
    pub label: String,
    pub url: String,
}

#[derive(Clone)]
pub struct SeriesAlternateTitleRecord {
    pub label: String,
    pub title: String,
}

pub struct SeriesMetadataUpdateRecord {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
}

pub async fn load_persisted_series_resource(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<PersistedSeriesResourceRecord>, String> {
    let row = sqlx::query(
        r#"SELECT s.LIBRARY_ID, sm.AGE_RATING,
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
         WHERE s.ID = ?
         GROUP BY s.LIBRARY_ID, sm.AGE_RATING"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted series resource: {error}"))?;

    Ok(row.map(|row| PersistedSeriesResourceRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(clamp_kotlin_int_u32),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
    }))
}

pub async fn load_series_id_by_sorted_position(
    pool: &SqlitePool,
    index: usize,
) -> Result<Option<String>, String> {
    let row = sqlx::query(
        r#"SELECT s.ID AS ID
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE s.DELETED_DATE IS NULL
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
         LIMIT 1
         OFFSET ?"#,
    )
    .bind((index - 1) as i64)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query remapped series id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("ID")))
}

pub async fn load_persisted_series_detail(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<PersistedSeriesDetailRecord>, String> {
    let row = sqlx::query(
        r#"SELECT s.ID AS ID, s.LIBRARY_ID AS LIBRARY_ID, s.NAME AS NAME, s.URL AS URL,
                s.CREATED_DATE AS CREATED_DATE, s.LAST_MODIFIED_DATE AS LAST_MODIFIED_DATE,
                CAST(s.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED, s.ONESHOT AS ONESHOT,
                s.DELETED_DATE AS DELETED_DATE, COALESCE(sm.STATUS, 'ONGOING') AS STATUS,
                COALESCE(sm.TITLE, s.NAME) AS TITLE,
                COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
                COALESCE(sm.SUMMARY, '') AS SUMMARY,
                COALESCE(sm.READING_DIRECTION, '') AS READING_DIRECTION,
                COALESCE(sm.PUBLISHER, '') AS PUBLISHER, sm.AGE_RATING AS AGE_RATING,
                COALESCE(sm.LANGUAGE, '') AS LANGUAGE,
                COALESCE(sm.CREATED_DATE, s.CREATED_DATE) AS METADATA_CREATED,
                COALESCE(sm.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
         WHERE s.ID = ?
         GROUP BY s.ID, s.LIBRARY_ID, s.NAME, s.URL, s.CREATED_DATE, s.LAST_MODIFIED_DATE,
                  s.FILE_LAST_MODIFIED, s.ONESHOT, s.DELETED_DATE, sm.STATUS, sm.TITLE,
                  sm.SUMMARY, sm.READING_DIRECTION, sm.PUBLISHER, sm.AGE_RATING, sm.LANGUAGE,
                  METADATA_CREATED, METADATA_LAST_MODIFIED"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted series detail: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let books_count = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*)
         FROM BOOK
         WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query persisted series books count: {error}"))?;

    Ok(Some(PersistedSeriesDetailRecord {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        name: row.get::<String, _>("NAME"),
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
            .map(clamp_kotlin_int_u32),
        language: row.get::<String, _>("LANGUAGE"),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<bool, _>("ONESHOT"),
    }))
}

pub async fn load_persisted_series_collections(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Vec<PersistedSeriesCollectionRecord>, String> {
    let rows = sqlx::query(
        r#"SELECT c.ID, c.NAME, c.ORDERED, c.CREATED_DATE, c.LAST_MODIFIED_DATE
         FROM COLLECTION c
         JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID
         WHERE cs.SERIES_ID = ?
         ORDER BY c.NAME COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted series collections: {error}"))?;

    let mut collections = Vec::with_capacity(rows.len());
    for row in rows {
        let collection_id = row.get::<String, _>("ID");
        let series_ids_rows = sqlx::query(
            r#"SELECT SERIES_ID
             FROM COLLECTION_SERIES
             WHERE COLLECTION_ID = ?
             ORDER BY NUMBER ASC"#,
        )
        .bind(collection_id.clone())
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query persisted collection series ids: {error}"))?;

        collections.push(PersistedSeriesCollectionRecord {
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
    pool: &SqlitePool,
    series_id: &str,
) -> Result<Option<ExistingSeriesMetadataRecord>, String> {
    let row = sqlx::query(
        r#"SELECT STATUS, STATUS_LOCK, TITLE, TITLE_LOCK, TITLE_SORT, TITLE_SORT_LOCK, SUMMARY,
                SUMMARY_LOCK, READING_DIRECTION, READING_DIRECTION_LOCK, PUBLISHER,
                PUBLISHER_LOCK, AGE_RATING, AGE_RATING_LOCK, LANGUAGE, LANGUAGE_LOCK,
                GENRES_LOCK, TAGS_LOCK, TOTAL_BOOK_COUNT, TOTAL_BOOK_COUNT_LOCK,
                SHARING_LABELS_LOCK, LINKS_LOCK, ALTERNATE_TITLES_LOCK
         FROM SERIES_METADATA
         WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query existing series metadata: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let genres = sqlx::query(
        r#"SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY GENRE COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing series metadata genres: {error}"))?
    .into_iter()
    .map(|row| row.get::<String, _>("GENRE"))
    .collect::<Vec<_>>();

    let tags = sqlx::query(
        r#"SELECT TAG FROM SERIES_METADATA_TAG WHERE SERIES_ID = ? ORDER BY TAG COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing series metadata tags: {error}"))?
    .into_iter()
    .map(|row| row.get::<String, _>("TAG"))
    .collect::<Vec<_>>();

    let sharing_labels = sqlx::query(
        r#"SELECT LABEL FROM SERIES_METADATA_SHARING
             WHERE SERIES_ID = ?
             ORDER BY LABEL COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing series metadata sharing labels: {error}"))?
    .into_iter()
    .map(|row| row.get::<String, _>("LABEL"))
    .collect::<Vec<_>>();

    let links = sqlx::query(
        r#"SELECT LABEL, URL FROM SERIES_METADATA_LINK
             WHERE SERIES_ID = ?
             ORDER BY LABEL COLLATE NOCASE ASC, URL ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing series metadata links: {error}"))?
    .into_iter()
    .map(|row| SeriesMetadataLinkRecord {
        label: row.get::<String, _>("LABEL"),
        url: row.get::<String, _>("URL"),
    })
    .collect::<Vec<_>>();

    let alternate_titles = sqlx::query(
        r#"SELECT LABEL, TITLE FROM SERIES_METADATA_ALTERNATE_TITLE
             WHERE SERIES_ID = ?
             ORDER BY LABEL COLLATE NOCASE ASC, TITLE COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query existing series metadata alternate titles: {error}"))?
    .into_iter()
    .map(|row| SeriesAlternateTitleRecord {
        label: row.get::<String, _>("LABEL"),
        title: row.get::<String, _>("TITLE"),
    })
    .collect::<Vec<_>>();

    Ok(Some(ExistingSeriesMetadataRecord {
        status: row.get::<String, _>("STATUS"),
        status_lock: row.get::<bool, _>("STATUS_LOCK"),
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<bool, _>("TITLE_LOCK"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        title_sort_lock: row.get::<bool, _>("TITLE_SORT_LOCK"),
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<bool, _>("SUMMARY_LOCK"),
        reading_direction: row.get::<Option<String>, _>("READING_DIRECTION"),
        reading_direction_lock: row.get::<bool, _>("READING_DIRECTION_LOCK"),
        publisher: row.get::<String, _>("PUBLISHER"),
        publisher_lock: row.get::<bool, _>("PUBLISHER_LOCK"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(clamp_kotlin_int_u32),
        age_rating_lock: row.get::<bool, _>("AGE_RATING_LOCK"),
        language: row.get::<String, _>("LANGUAGE"),
        language_lock: row.get::<bool, _>("LANGUAGE_LOCK"),
        genres,
        genres_lock: row.get::<bool, _>("GENRES_LOCK"),
        tags,
        tags_lock: row.get::<bool, _>("TAGS_LOCK"),
        total_book_count: row
            .get::<Option<i64>, _>("TOTAL_BOOK_COUNT")
            .map(clamp_kotlin_int_u32),
        total_book_count_lock: row.get::<bool, _>("TOTAL_BOOK_COUNT_LOCK"),
        sharing_labels,
        sharing_labels_lock: row.get::<bool, _>("SHARING_LABELS_LOCK"),
        links,
        links_lock: row.get::<bool, _>("LINKS_LOCK"),
        alternate_titles,
        alternate_titles_lock: row.get::<bool, _>("ALTERNATE_TITLES_LOCK"),
    }))
}

pub async fn persist_series_metadata_update(
    pool: &SqlitePool,
    series_id: &str,
    update: SeriesMetadataUpdateRecord,
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin series metadata update tx: {error}"))?;

    let result = sqlx::query(
        r#"UPDATE SERIES_METADATA
         SET STATUS = ?, STATUS_LOCK = ?, TITLE = ?, TITLE_LOCK = ?, TITLE_SORT = ?,
             TITLE_SORT_LOCK = ?, SUMMARY = ?, SUMMARY_LOCK = ?, READING_DIRECTION = ?,
             READING_DIRECTION_LOCK = ?, PUBLISHER = ?, PUBLISHER_LOCK = ?, AGE_RATING = ?,
             AGE_RATING_LOCK = ?, LANGUAGE = ?, LANGUAGE_LOCK = ?, GENRES_LOCK = ?,
             TAGS_LOCK = ?, TOTAL_BOOK_COUNT = ?, TOTAL_BOOK_COUNT_LOCK = ?,
             SHARING_LABELS_LOCK = ?, LINKS_LOCK = ?, ALTERNATE_TITLES_LOCK = ?,
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
         WHERE SERIES_ID = ?"#,
    )
    .bind(&update.status)
    .bind(update.status_lock)
    .bind(&update.title)
    .bind(update.title_lock)
    .bind(&update.title_sort)
    .bind(update.title_sort_lock)
    .bind(&update.summary)
    .bind(update.summary_lock)
    .bind(update.reading_direction.as_deref())
    .bind(update.reading_direction_lock)
    .bind(&update.publisher)
    .bind(update.publisher_lock)
    .bind(update.age_rating.map(i64::from))
    .bind(update.age_rating_lock)
    .bind(&update.language)
    .bind(update.language_lock)
    .bind(update.genres_lock)
    .bind(update.tags_lock)
    .bind(update.total_book_count.map(i64::from))
    .bind(update.total_book_count_lock)
    .bind(update.sharing_labels_lock)
    .bind(update.links_lock)
    .bind(update.alternate_titles_lock)
    .bind(series_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("persist series metadata update: {error}"))?;

    if result.rows_affected() == 0 {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback series metadata update tx: {error}"))?;
        return Ok(false);
    }

    replace_series_metadata_strings(
        &mut tx,
        "SERIES_METADATA_GENRE",
        "GENRE",
        series_id,
        &update.genres,
    )
    .await?;
    replace_series_metadata_strings(
        &mut tx,
        "SERIES_METADATA_TAG",
        "TAG",
        series_id,
        &update.tags,
    )
    .await?;
    replace_series_metadata_strings(
        &mut tx,
        "SERIES_METADATA_SHARING",
        "LABEL",
        series_id,
        &update.sharing_labels,
    )
    .await?;
    replace_series_metadata_links(&mut tx, series_id, &update.links).await?;
    replace_series_metadata_alternate_titles(&mut tx, series_id, &update.alternate_titles).await?;

    tx.commit()
        .await
        .map_err(|error| format!("commit series metadata update tx: {error}"))?;

    Ok(true)
}

async fn replace_series_metadata_strings(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &str,
    value_column: &str,
    series_id: &str,
    values: &[String],
) -> Result<(), String> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"DELETE FROM {table} WHERE SERIES_ID = ?"#
    )))
    .bind(series_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| format!("clear {table} during series metadata update: {error}"))?;

    for value in values {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"INSERT INTO {table} (SERIES_ID, {value_column}) VALUES (?, ?)"#
        )))
        .bind(series_id)
        .bind(value)
        .execute(&mut **tx)
        .await
        .map_err(|error| format!("insert {table} during series metadata update: {error}"))?;
    }

    Ok(())
}

fn clamp_kotlin_int_u32(value: i64) -> u32 {
    value.clamp(0, i64::from(i32::MAX)) as u32
}

async fn replace_series_metadata_alternate_titles(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    series_id: &str,
    titles: &[SeriesAlternateTitleRecord],
) -> Result<(), String> {
    sqlx::query(r#"DELETE FROM SERIES_METADATA_ALTERNATE_TITLE WHERE SERIES_ID = ?"#)
        .bind(series_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            format!("clear SERIES_METADATA_ALTERNATE_TITLE during series metadata update: {error}")
        })?;

    for title in titles {
        sqlx::query(
            r#"INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE) VALUES (?, ?, ?)"#,
        )
        .bind(series_id)
        .bind(&title.label)
        .bind(&title.title)
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            format!("insert SERIES_METADATA_ALTERNATE_TITLE during series metadata update: {error}")
        })?;
    }

    Ok(())
}

async fn replace_series_metadata_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    series_id: &str,
    links: &[SeriesMetadataLinkRecord],
) -> Result<(), String> {
    sqlx::query(r#"DELETE FROM SERIES_METADATA_LINK WHERE SERIES_ID = ?"#)
        .bind(series_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            format!("clear SERIES_METADATA_LINK during series metadata update: {error}")
        })?;

    for link in links {
        sqlx::query(r#"INSERT INTO SERIES_METADATA_LINK (SERIES_ID, LABEL, URL) VALUES (?, ?, ?)"#)
            .bind(series_id)
            .bind(&link.label)
            .bind(&link.url)
            .execute(&mut **tx)
            .await
            .map_err(|error| {
                format!("insert SERIES_METADATA_LINK during series metadata update: {error}")
            })?;
    }

    Ok(())
}

pub async fn refresh_series_after_metadata_update(
    pool: &SqlitePool,
    series_id: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"UPDATE SERIES_METADATA
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
         WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .execute(pool)
    .await
    .map_err(|error| format!("refresh series metadata timestamp: {error}"))?;

    sqlx::query(
        r#"UPDATE SERIES
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
         WHERE ID = ?"#,
    )
    .bind(series_id)
    .execute(pool)
    .await
    .map_err(|error| format!("refresh series row timestamp: {error}"))?;

    Ok(())
}
