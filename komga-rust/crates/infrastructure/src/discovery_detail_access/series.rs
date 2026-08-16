use anyhow::Context;
use sqlx::{Row, SqlitePool};

use komga_application::discovery::{
    ExistingSeriesMetadataRecord, PersistedSeriesCollectionRecord, PersistedSeriesDetailRecord,
    PersistedSeriesResourceRecord, SeriesAlternateTitleRecord, SeriesMetadataLinkRecord,
    SeriesMetadataUpdateRecord, SeriesReadingDirection,
};
use komga_domain::discovery::SeriesStatus;

use crate::parsing::clamp_kotlin_int_u32;

pub(super) async fn load_persisted_series_resource(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<PersistedSeriesResourceRecord>> {
    let row = sqlx::query(
        r#"SELECT s.LIBRARY_ID, sm.AGE_RATING,
                COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                          FROM (SELECT DISTINCT sms.LABEL AS LABEL
                                FROM SERIES_METADATA_SHARING sms
                                WHERE sms.SERIES_ID = s.ID)), '') AS SHARING_LABELS
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE s.ID = ?
         GROUP BY s.LIBRARY_ID, sm.AGE_RATING"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .context("query persisted series resource")?;

    Ok(row.map(|row| PersistedSeriesResourceRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(clamp_kotlin_int_u32),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
    }))
}

pub(super) async fn load_series_id_by_sorted_position(
    pool: &SqlitePool,
    index: usize,
) -> anyhow::Result<Option<String>> {
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
    .context("query remapped series id")?;

    Ok(row.map(|row| row.get::<String, _>("ID")))
}

pub(super) async fn load_persisted_series_detail(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<PersistedSeriesDetailRecord>> {
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
                COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                          FROM (SELECT DISTINCT sms.LABEL AS LABEL
                                FROM SERIES_METADATA_SHARING sms
                                WHERE sms.SERIES_ID = s.ID)), '') AS SHARING_LABELS
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE s.ID = ?
         GROUP BY s.ID, s.LIBRARY_ID, s.NAME, s.URL, s.CREATED_DATE, s.LAST_MODIFIED_DATE,
                  s.FILE_LAST_MODIFIED, s.ONESHOT, s.DELETED_DATE, sm.STATUS, sm.TITLE,
                  sm.SUMMARY, sm.READING_DIRECTION, sm.PUBLISHER, sm.AGE_RATING, sm.LANGUAGE,
                  METADATA_CREATED, METADATA_LAST_MODIFIED"#,
    )
    .bind(series_id)
    .fetch_optional(pool)
    .await
    .context("query persisted series detail")?;

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
    .context("query persisted series books count")?;

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
        status: SeriesStatus::parse(&row.get::<String, _>("STATUS"))
            .unwrap_or(SeriesStatus::Ongoing),
        summary: row.get::<String, _>("SUMMARY"),
        reading_direction: SeriesReadingDirection::parse(
            &row.get::<String, _>("READING_DIRECTION"),
        ),
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

pub(super) async fn load_persisted_series_collections(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Vec<PersistedSeriesCollectionRecord>> {
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
    .context("query persisted series collections")?;

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
        .context("query persisted collection series ids")?;

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

pub(super) async fn load_existing_series_metadata(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<Option<ExistingSeriesMetadataRecord>> {
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
    .context("query existing series metadata")?;

    let Some(row) = row else {
        return Ok(None);
    };

    let genres = sqlx::query(
        r#"SELECT GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ? ORDER BY GENRE COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query existing series metadata genres")?
    .into_iter()
    .map(|row| row.get::<String, _>("GENRE"))
    .collect::<Vec<_>>();

    let tags = sqlx::query(
        r#"SELECT TAG FROM SERIES_METADATA_TAG WHERE SERIES_ID = ? ORDER BY TAG COLLATE NOCASE ASC"#,
    )
    .bind(series_id)
    .fetch_all(pool)
    .await
    .context("query existing series metadata tags")?
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
    .context("query existing series metadata sharing labels")?
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
    .context("query existing series metadata links")?
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
    .context("query existing series metadata alternate titles")?
    .into_iter()
    .map(|row| SeriesAlternateTitleRecord {
        label: row.get::<String, _>("LABEL"),
        title: row.get::<String, _>("TITLE"),
    })
    .collect::<Vec<_>>();

    Ok(Some(ExistingSeriesMetadataRecord {
        status: SeriesStatus::parse(&row.get::<String, _>("STATUS"))
            .unwrap_or(SeriesStatus::Ongoing),
        status_lock: row.get::<bool, _>("STATUS_LOCK"),
        title: row.get::<String, _>("TITLE"),
        title_lock: row.get::<bool, _>("TITLE_LOCK"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        title_sort_lock: row.get::<bool, _>("TITLE_SORT_LOCK"),
        summary: row.get::<String, _>("SUMMARY"),
        summary_lock: row.get::<bool, _>("SUMMARY_LOCK"),
        reading_direction: row
            .get::<Option<String>, _>("READING_DIRECTION")
            .as_deref()
            .and_then(SeriesReadingDirection::parse),
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

pub(super) async fn persist_series_metadata_update(
    pool: &SqlitePool,
    series_id: &str,
    update: SeriesMetadataUpdateRecord,
) -> anyhow::Result<bool> {
    let mut tx = pool
        .begin()
        .await
        .context("begin series metadata update tx")?;

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
    .bind(update.status.persisted_name())
    .bind(update.status_lock)
    .bind(&update.title)
    .bind(update.title_lock)
    .bind(&update.title_sort)
    .bind(update.title_sort_lock)
    .bind(&update.summary)
    .bind(update.summary_lock)
    .bind(update.reading_direction.map(|value| value.persisted_name()))
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
    .context("persist series metadata update")?;

    if result.rows_affected() == 0 {
        tx.rollback()
            .await
            .context("rollback series metadata update tx")?;
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
        .context("commit series metadata update tx")?;

    Ok(true)
}

async fn replace_series_metadata_strings(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &str,
    value_column: &str,
    series_id: &str,
    values: &[String],
) -> anyhow::Result<()> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        r#"DELETE FROM {table} WHERE SERIES_ID = ?"#
    )))
    .bind(series_id)
    .execute(&mut **tx)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!("clear {table} during series metadata update"))
    })?;

    for value in values {
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"INSERT INTO {table} (SERIES_ID, {value_column}) VALUES (?, ?)"#
        )))
        .bind(series_id)
        .bind(value)
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!("insert {table} during series metadata update"))
        })?;
    }

    Ok(())
}

async fn replace_series_metadata_alternate_titles(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    series_id: &str,
    titles: &[SeriesAlternateTitleRecord],
) -> anyhow::Result<()> {
    sqlx::query(r#"DELETE FROM SERIES_METADATA_ALTERNATE_TITLE WHERE SERIES_ID = ?"#)
        .bind(series_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error)
                .context("clear SERIES_METADATA_ALTERNATE_TITLE during series metadata update: ")
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
        .context("insert SERIES_METADATA_ALTERNATE_TITLE during series metadata update")?;
    }

    Ok(())
}

async fn replace_series_metadata_links(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    series_id: &str,
    links: &[SeriesMetadataLinkRecord],
) -> anyhow::Result<()> {
    sqlx::query(r#"DELETE FROM SERIES_METADATA_LINK WHERE SERIES_ID = ?"#)
        .bind(series_id)
        .execute(&mut **tx)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error)
                .context("clear SERIES_METADATA_LINK during series metadata update: ")
        })?;

    for link in links {
        sqlx::query(r#"INSERT INTO SERIES_METADATA_LINK (SERIES_ID, LABEL, URL) VALUES (?, ?, ?)"#)
            .bind(series_id)
            .bind(&link.label)
            .bind(&link.url)
            .execute(&mut **tx)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error)
                    .context("insert SERIES_METADATA_LINK during series metadata update: ")
            })?;
    }

    Ok(())
}

pub(super) async fn refresh_series_after_metadata_update(
    pool: &SqlitePool,
    series_id: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE SERIES_METADATA
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
         WHERE SERIES_ID = ?"#,
    )
    .bind(series_id)
    .execute(pool)
    .await
    .context("refresh series metadata timestamp")?;

    sqlx::query(
        r#"UPDATE SERIES
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
         WHERE ID = ?"#,
    )
    .bind(series_id)
    .execute(pool)
    .await
    .context("refresh series row timestamp")?;

    Ok(())
}
