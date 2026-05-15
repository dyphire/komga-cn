use super::*;

pub async fn load_persisted_series_summaries(
    pool: &SqlitePool,
) -> Result<Vec<SeriesSummary>, String> {
    let rows = fetch_persisted_series_summary_rows(pool, None)
        .await
        .map_err(|error| format!("query persisted series summaries: {error}"))?;

    Ok(rows.into_iter().map(map_series_summary).collect())
}

pub async fn load_persisted_series_summaries_by_ids(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<Vec<SeriesSummary>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let rows = fetch_persisted_series_summary_rows(pool, Some(ids))
        .await
        .map_err(|error| format!("query persisted series summaries by ids: {error}"))?;

    let mut rows_by_id: HashMap<String, SeriesSummary> = rows
        .into_iter()
        .map(map_series_summary)
        .map(|row| (row.id.clone(), row))
        .collect();

    Ok(ids.iter().filter_map(|id| rows_by_id.remove(id)).collect())
}

async fn fetch_persisted_series_summary_rows(
    pool: &sqlx::SqlitePool,
    ids: Option<&[String]>,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT s.ID,
                  s.LIBRARY_ID,
                  s.CREATED_DATE,
                  s.LAST_MODIFIED_DATE,
                  CAST(s.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  s.BOOK_COUNT,
                  s.DELETED_DATE,
                  CAST(COALESCE(s.ONESHOT, 0) AS INTEGER) AS ONESHOT,
                  COALESCE(sm.TITLE, s.NAME) AS TITLE,
                  COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
                  COALESCE(sm.STATUS, 'ONGOING') AS STATUS,
                  COALESCE(sm.SUMMARY, '') AS SUMMARY,
                  COALESCE(sm.READING_DIRECTION, '') AS READING_DIRECTION,
                  COALESCE(sm.PUBLISHER, '') AS PUBLISHER,
                  sm.AGE_RATING AS AGE_RATING,
                  sm.TOTAL_BOOK_COUNT AS TOTAL_BOOK_COUNT,
                  COALESCE(sm.LANGUAGE, '') AS LANGUAGE,
                  COALESCE(sm.CREATED_DATE, s.CREATED_DATE) AS METADATA_CREATED,
                  COALESCE(sm.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                  COALESCE(bma.RELEASE_DATE, NULL) AS BOOKS_METADATA_RELEASE_DATE,
                  COALESCE(bma.SUMMARY, '') AS BOOKS_METADATA_SUMMARY,
                   COALESCE(bma.SUMMARY_NUMBER, '') AS BOOKS_METADATA_SUMMARY_NUMBER,
                   COALESCE(bma.CREATED_DATE, s.CREATED_DATE) AS BOOKS_METADATA_CREATED,
                   COALESCE(bma.LAST_MODIFIED_DATE, s.LAST_MODIFIED_DATE) AS BOOKS_METADATA_LAST_MODIFIED,
                   s.NAME AS NAME,
                   COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
                  COALESCE(GROUP_CONCAT(DISTINCT smg.GENRE), '') AS GENRES,
                  COALESCE(GROUP_CONCAT(DISTINCT smt.TAG), '') AS TAGS,
                  COALESCE(
                    GROUP_CONCAT(
                      DISTINCT CASE
                        WHEN smat.LABEL IS NULL OR smat.LABEL = '' THEN smat.TITLE
                        ELSE smat.LABEL || '::' || smat.TITLE
                      END
                    ),
                    ''
                  ) AS ALTERNATE_TITLES,
                  COALESCE(
                    GROUP_CONCAT(
                      DISTINCT CASE
                        WHEN bmaa.ROLE IS NULL OR bmaa.ROLE = '' THEN bmaa.NAME
                        ELSE bmaa.NAME || '::' || bmaa.ROLE
                      END
                    ),
                    ''
                  ) AS BOOKS_METADATA_AUTHORS,
                  COALESCE(GROUP_CONCAT(DISTINCT bmat.TAG), '') AS BOOKS_METADATA_TAGS
           FROM SERIES s
           LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_GENRE smg ON smg.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_TAG smt ON smt.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_ALTERNATE_TITLE smat ON smat.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_AGGREGATION_AUTHOR bmaa ON bmaa.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_AGGREGATION_TAG bmat ON bmat.SERIES_ID = s.ID"#,
    );

    if let Some(ids) = ids.filter(|ids| !ids.is_empty()) {
        query.push(" WHERE s.ID IN (");
        let mut separated = query.separated(",");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }

    query.push(
        r#" GROUP BY s.ID,
                    s.LIBRARY_ID,
                    s.CREATED_DATE,
                    s.LAST_MODIFIED_DATE,
                    s.FILE_LAST_MODIFIED,
                    s.BOOK_COUNT,
                    s.DELETED_DATE,
                    s.ONESHOT,
                    sm.TITLE,
                    sm.TITLE_SORT,
                    sm.STATUS,
                    sm.SUMMARY,
                    sm.READING_DIRECTION,
                    sm.PUBLISHER,
                    sm.AGE_RATING,
                    sm.TOTAL_BOOK_COUNT,
                    sm.LANGUAGE,
                    sm.CREATED_DATE,
                    sm.LAST_MODIFIED_DATE,
                    bma.RELEASE_DATE,
                    bma.SUMMARY,
                    bma.SUMMARY_NUMBER,
                    bma.CREATED_DATE,
                    bma.LAST_MODIFIED_DATE,
                    s.NAME"#,
    );

    query.build().fetch_all(pool).await
}

pub async fn load_persisted_series_count(pool: &SqlitePool) -> Result<usize, String> {
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("query persisted series count: {error}"))?;
    Ok(row.get::<i64, _>("COUNT").max(0) as usize)
}

fn map_series_summary(row: sqlx::sqlite::SqliteRow) -> SeriesSummary {
    SeriesSummary {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        name: row.get::<String, _>("NAME"),
        title: row.get::<String, _>("TITLE"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        labels: common::parse_csv_values(&row.get::<String, _>("LABELS")),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        books_count: row.get::<i64, _>("BOOK_COUNT").max(0) as u64,
        books_read_count: 0,
        books_unread_count: row.get::<i64, _>("BOOK_COUNT").max(0) as u64,
        books_in_progress_count: 0,
        status: row.get::<String, _>("STATUS"),
        summary: row.get::<String, _>("SUMMARY"),
        reading_direction: row.get::<String, _>("READING_DIRECTION"),
        publisher: row.get::<String, _>("PUBLISHER"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value as u16),
        language: row.get::<String, _>("LANGUAGE"),
        genres: common::parse_csv_values(&row.get::<String, _>("GENRES")),
        tags: common::parse_csv_values(&row.get::<String, _>("TAGS")),
        alternate_titles: common::parse_csv_values(&row.get::<String, _>("ALTERNATE_TITLES")),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        books_metadata_authors: common::parse_csv_values(
            &row.get::<String, _>("BOOKS_METADATA_AUTHORS"),
        ),
        books_metadata_tags: common::parse_csv_values(&row.get::<String, _>("BOOKS_METADATA_TAGS")),
        books_metadata_release_date: row.get::<Option<String>, _>("BOOKS_METADATA_RELEASE_DATE"),
        books_metadata_summary: row.get::<String, _>("BOOKS_METADATA_SUMMARY"),
        books_metadata_summary_number: row.get::<String, _>("BOOKS_METADATA_SUMMARY_NUMBER"),
        books_metadata_created: row.get::<String, _>("BOOKS_METADATA_CREATED"),
        books_metadata_last_modified: row.get::<String, _>("BOOKS_METADATA_LAST_MODIFIED"),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<i64, _>("ONESHOT") != 0,
    }
}

pub async fn persisted_series_exist(pool: &SqlitePool) -> Result<bool, String> {
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM SERIES")
        .fetch_one(pool)
        .await
        .map_err(|error| format!("query persisted series count: {error}"))?;
    Ok(row.get::<i64, _>("COUNT") > 0)
}
