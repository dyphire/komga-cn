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

    let rows = if let Some(user_id) = user_id {
        sqlx::query(
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
                      COALESCE(bm.TITLE, b.NAME) AS TITLE,
                      bm.NUMBER_SORT AS NUMBER_SORT,
                      bm.RELEASE_DATE AS RELEASE_DATE,
                      COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                      COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                      COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                      COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
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
               LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
               LEFT JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
               LEFT JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID
               LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID
                                      AND rp.USER_ID = ?
               GROUP BY b.ID,
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
                        rp.COMPLETED"#,
        )
        .bind(user_id)
        .fetch_all(&pool)
        .await
    } else {
        sqlx::query(
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
                      COALESCE(bm.TITLE, b.NAME) AS TITLE,
                      bm.NUMBER_SORT AS NUMBER_SORT,
                      bm.RELEASE_DATE AS RELEASE_DATE,
                      COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                      COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                      COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                      COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS,
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
               LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
               LEFT JOIN BOOK_METADATA_TAG bmt ON bmt.BOOK_ID = b.ID
               LEFT JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID
               GROUP BY b.ID,
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
                        m.PAGE_COUNT"#,
        )
        .fetch_all(&pool)
        .await
    }
    .map_err(|error| format!("query persisted book summaries: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BookSummary {
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
            metadata_tags: common::parse_csv_values(&row.get::<String, _>("METADATA_TAGS")),
            metadata_authors: common::parse_csv_values(&row.get::<String, _>("METADATA_AUTHORS")),
        })
        .collect())
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
