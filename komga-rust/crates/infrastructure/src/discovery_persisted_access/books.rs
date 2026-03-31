use super::*;

pub async fn load_book_poster_summaries(
    database_file: &FsPath,
) -> Result<HashMap<String, Vec<BookPosterSummary>>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book poster db: {error}"))?;

    let rows = sqlx::query(
        "SELECT BOOK_ID, TYPE, SELECTED \
         FROM THUMBNAIL_BOOK",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query book posters: {error}"))?;

    let mut posters: HashMap<String, Vec<BookPosterSummary>> = HashMap::new();
    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        let poster = BookPosterSummary {
            thumbnail_type: row.get::<String, _>("TYPE"),
            selected: row.get::<i64, _>("SELECTED") != 0,
        };
        posters.entry(book_id).or_default().push(poster);
    }

    Ok(posters)
}

pub async fn load_persisted_book_summaries(
    database_file: &FsPath,
    user_id: Option<&str>,
) -> Result<Vec<BookSummary>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books db: {error}"))?;

    let rows = fetch_persisted_book_summary_rows(&pool, user_id, None)
        .await
        .map_err(|error| format!("query persisted book summaries: {error}"))?;

    Ok(rows.into_iter().map(map_book_summary).collect())
}

pub async fn load_persisted_book_summaries_by_ids(
    database_file: &FsPath,
    user_id: Option<&str>,
    ids: &[String],
) -> Result<Vec<BookSummary>, String> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books db for ids query: {error}"))?;

    let rows = fetch_persisted_book_summary_rows(&pool, user_id, Some(ids))
        .await
        .map_err(|error| format!("query persisted book summaries by ids: {error}"))?;

    let mut rows_by_id: HashMap<String, BookSummary> = rows
        .into_iter()
        .map(map_book_summary)
        .map(|row| (row.id.clone(), row))
        .collect();

    Ok(ids.iter().filter_map(|id| rows_by_id.remove(id)).collect())
}

async fn fetch_persisted_book_summary_rows(
    pool: &sqlx::SqlitePool,
    user_id: Option<&str>,
    ids: Option<&[String]>,
) -> Result<Vec<sqlx::sqlite::SqliteRow>, sqlx::Error> {
    let mut query = QueryBuilder::<Sqlite>::new(book_summary_select_sql(user_id.is_some()));

    if let Some(user_id) = user_id {
        query.push_bind(user_id);
    }

    if let Some(ids) = ids.filter(|ids| !ids.is_empty()) {
        query.push(" WHERE b.ID IN (");
        let mut separated = query.separated(",");
        for id in ids {
            separated.push_bind(id);
        }
        separated.push_unseparated(")");
    }

    query.push(book_summary_group_by_sql(user_id.is_some()));
    query.build().fetch_all(pool).await
}

fn book_summary_select_sql(include_read_progress: bool) -> &'static str {
    if include_read_progress {
        r#"SELECT b.ID,
                  b.SERIES_ID,
                  b.LIBRARY_ID,
                  b.URL,
                  b.CREATED_DATE,
                  b.LAST_MODIFIED_DATE,
                  CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  b.FILE_SIZE,
                  s.ONESHOT AS ONESHOT,
                  b.DELETED_DATE,
                   COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                   sm.LANGUAGE AS LANGUAGE,
                   sm.PUBLISHER AS PUBLISHER,
                   sm.AGE_RATING AS AGE_RATING,
                   COALESCE(bm.TITLE, b.NAME) AS TITLE,
                  bm.NUMBER_SORT AS NUMBER_SORT,
                  bm.RELEASE_DATE AS RELEASE_DATE,
                  COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                  COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                  COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                   COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
                   COALESCE(GROUP_CONCAT(DISTINCT smg.GENRE), '') AS GENRES,
                   COALESCE(GROUP_CONCAT(DISTINCT bmt.TAG), '') AS METADATA_TAGS,
                  COALESCE(
                    GROUP_CONCAT(
                      DISTINCT CASE
                        WHEN bma.ROLE IS NULL OR bma.ROLE = '' THEN bma.NAME
                        ELSE bma.NAME || '::' || bma.ROLE
                      END
                    ),
                    ''
                  ) AS METADATA_AUTHORS,
                  CASE
                    WHEN rp.BOOK_ID IS NULL THEN 'unread'
                    WHEN rp.COMPLETED = 1 THEN 'read'
                    ELSE 'in_progress'
                  END AS READ_STATUS
           FROM BOOK b
           JOIN SERIES s ON s.ID = b.SERIES_ID
           LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
           LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
           LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_GENRE smg ON smg.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
           LEFT JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID
           LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID
                                  AND rp.USER_ID = "#
    } else {
        r#"SELECT b.ID,
                  b.SERIES_ID,
                  b.LIBRARY_ID,
                  b.URL,
                  b.CREATED_DATE,
                  b.LAST_MODIFIED_DATE,
                  CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  b.FILE_SIZE,
                  s.ONESHOT AS ONESHOT,
                  b.DELETED_DATE,
                   COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                   sm.LANGUAGE AS LANGUAGE,
                   sm.PUBLISHER AS PUBLISHER,
                   sm.AGE_RATING AS AGE_RATING,
                   COALESCE(bm.TITLE, b.NAME) AS TITLE,
                  bm.NUMBER_SORT AS NUMBER_SORT,
                  bm.RELEASE_DATE AS RELEASE_DATE,
                  COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                  COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                  COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                   COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
                   COALESCE(GROUP_CONCAT(DISTINCT smg.GENRE), '') AS GENRES,
                   COALESCE(GROUP_CONCAT(DISTINCT bmt.TAG), '') AS METADATA_TAGS,
                  COALESCE(
                    GROUP_CONCAT(
                      DISTINCT CASE
                        WHEN bma.ROLE IS NULL OR bma.ROLE = '' THEN bma.NAME
                        ELSE bma.NAME || '::' || bma.ROLE
                      END
                    ),
                    ''
                  ) AS METADATA_AUTHORS,
                  'unread' AS READ_STATUS
           FROM BOOK b
           JOIN SERIES s ON s.ID = b.SERIES_ID
           LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
           LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
           LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_GENRE smg ON smg.SERIES_ID = s.ID
           LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
           LEFT JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
           LEFT JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID"#
    }
}

fn book_summary_group_by_sql(include_read_progress: bool) -> &'static str {
    if include_read_progress {
        r#" GROUP BY b.ID,
                    b.SERIES_ID,
                    b.LIBRARY_ID,
                    b.URL,
                    b.CREATED_DATE,
                    b.LAST_MODIFIED_DATE,
                    b.FILE_LAST_MODIFIED,
                    b.FILE_SIZE,
                    s.ONESHOT,
                    b.DELETED_DATE,
                    sm.TITLE,
                    s.NAME,
                    bm.TITLE,
                    b.NAME,
                    bm.NUMBER_SORT,
                    bm.RELEASE_DATE,
                    m.STATUS,
                    m.MEDIA_TYPE,
                    m.PAGE_COUNT,
                    rp.BOOK_ID,
                    rp.COMPLETED"#
    } else {
        r#" GROUP BY b.ID,
                    b.SERIES_ID,
                    b.LIBRARY_ID,
                    b.URL,
                    b.CREATED_DATE,
                    b.LAST_MODIFIED_DATE,
                    b.FILE_LAST_MODIFIED,
                    b.FILE_SIZE,
                    s.ONESHOT,
                    b.DELETED_DATE,
                    sm.TITLE,
                    s.NAME,
                    bm.TITLE,
                    b.NAME,
                    bm.NUMBER_SORT,
                    bm.RELEASE_DATE,
                    m.STATUS,
                    m.MEDIA_TYPE,
                    m.PAGE_COUNT"#
    }
}

pub async fn load_persisted_book_count(database_file: &FsPath) -> Result<usize, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books db for count: {error}"))?;
    let row = sqlx::query("SELECT COUNT(*) AS COUNT FROM BOOK")
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("query persisted book count: {error}"))?;
    Ok(row.get::<i64, _>("COUNT").max(0) as usize)
}

fn map_book_summary(row: sqlx::sqlite::SqliteRow) -> BookSummary {
    BookSummary {
        id: row.get::<String, _>("ID"),
        series_id: row.get::<String, _>("SERIES_ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        media_status: row.get::<String, _>("MEDIA_STATUS"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        read_status: row.get::<String, _>("READ_STATUS"),
        metadata_number_sort: row.get::<Option<f64>, _>("NUMBER_SORT"),
        metadata_release_date: row.get::<Option<String>, _>("RELEASE_DATE"),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<bool, _>("ONESHOT"),
        genres: common::parse_csv_values(&row.get::<String, _>("GENRES")),
        language: row.get::<Option<String>, _>("LANGUAGE"),
        publisher: row.get::<Option<String>, _>("PUBLISHER"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value.max(0) as u16),
        metadata_tags: common::parse_csv_values(&row.get::<String, _>("METADATA_TAGS")),
        metadata_authors: common::parse_csv_values(&row.get::<String, _>("METADATA_AUTHORS")),
    }
}

pub async fn persisted_books_exist(database_file: &FsPath) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted books db: {error}"))?;
    let row = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM BOOK \
         WHERE DELETED_DATE IS NULL",
    )
    .fetch_one(&pool)
    .await
    .map_err(|error| format!("query persisted books count: {error}"))?;

    Ok(row.get::<i64, _>("COUNT") > 0)
}
