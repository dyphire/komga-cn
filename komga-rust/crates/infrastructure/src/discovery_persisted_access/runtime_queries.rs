use super::*;

pub async fn load_persisted_ondeck_books(
    database_file: &FsPath,
    user_id: &str,
) -> Result<Vec<BookBrowseEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books ondeck db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.SERIES_ID, \
                b.NUMBER \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.SERIES_ID IN (SELECT DISTINCT b_done.SERIES_ID \
         FROM BOOK b_done \
         JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID \
         WHERE rp_done.USER_ID = ? \
         AND rp_done.COMPLETED = 1) \
         AND b.SERIES_ID NOT IN (SELECT DISTINCT b_prog.SERIES_ID \
         FROM BOOK b_prog \
         JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID \
         WHERE rp_prog.USER_ID = ? \
         AND rp_prog.COMPLETED = 0) \
         AND NOT EXISTS (SELECT 1 \
         FROM READ_PROGRESS rp_seen \
         WHERE rp_seen.BOOK_ID = b.ID \
         AND rp_seen.USER_ID = ? \
         AND rp_seen.COMPLETED = 1) \
         ORDER BY b.SERIES_ID ASC, b.NUMBER ASC",
    )
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted books ondeck: {error}"))?;

    let mut first_per_series = BTreeMap::<String, BookBrowseEntry>::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        first_per_series
            .entry(series_id)
            .or_insert_with(|| BookBrowseEntry {
                id: row.get::<String, _>("ID"),
                library_id: row.get::<String, _>("LIBRARY_ID"),
                name: row.get::<String, _>("NAME"),
                title: row.get::<String, _>("TITLE"),
            });
    }

    Ok(first_per_series.into_values().collect())
}

pub async fn load_persisted_duplicate_books(
    database_file: &FsPath,
) -> Result<Vec<BookBrowseEntry>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open books duplicates db: {error}"))?;

    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.FILE_HASH IS NOT NULL \
         AND TRIM(b.FILE_HASH) != '' \
         AND b.FILE_HASH IN (SELECT FILE_HASH \
         FROM BOOK \
         WHERE FILE_HASH IS NOT NULL \
         AND TRIM(FILE_HASH) != '' \
         GROUP BY FILE_HASH \
         HAVING COUNT(*) > 1) \
         ORDER BY b.FILE_HASH ASC, b.NUMBER ASC, b.ID ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted books duplicates: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| BookBrowseEntry {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            name: row.get::<String, _>("NAME"),
            title: row.get::<String, _>("TITLE"),
        })
        .collect())
}

pub async fn load_persisted_book_tags(
    database_file: &FsPath,
    scope: Option<&BookTagsScope>,
) -> Result<Vec<String>, String> {
    let Some(scope) = scope else {
        return Ok(vec![]);
    };

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book tags db: {error}"))?;

    let rows = match scope {
        BookTagsScope::Series(series_id) => {
            sqlx::query(
                "SELECT bt.TAG \
                 FROM BOOK_METADATA_TAG bt \
                 JOIN BOOK b ON b.ID = bt.BOOK_ID \
                 WHERE b.SERIES_ID = ? \
                 ORDER BY lower(bt.TAG), bt.TAG, b.ID",
            )
            .bind(series_id)
            .fetch_all(&pool)
            .await
        }
        BookTagsScope::Library(library_id) => {
            sqlx::query(
                "SELECT bt.TAG \
                 FROM BOOK_METADATA_TAG bt \
                 JOIN BOOK b ON b.ID = bt.BOOK_ID \
                 WHERE b.LIBRARY_ID = ? \
                 ORDER BY lower(bt.TAG), bt.TAG, b.ID",
            )
            .bind(library_id)
            .fetch_all(&pool)
            .await
        }
    }
    .map_err(|error| format!("query persisted book tags: {error}"))?;

    let mut tags = Vec::with_capacity(rows.len());
    let mut seen = BTreeSet::new();
    for row in rows {
        let tag = row.get::<String, _>("TAG");
        if seen.insert(tag.clone()) {
            tags.push(tag);
        }
    }

    Ok(tags)
}

pub async fn persisted_utc_date_minus_days(
    database_file: &FsPath,
    days: i64,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted date db: {error}"))?;

    let modifier = if days >= 0 {
        format!("-{days} days")
    } else {
        format!("+{} days", days.saturating_abs())
    };

    let row = sqlx::query("SELECT date('now', ?) AS CUTOFF")
        .bind(modifier)
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("query persisted utc cutoff date: {error}"))?;

    Ok(row.get::<Option<String>, _>("CUTOFF"))
}

pub async fn load_series_read_progress_counts(
    database_file: &FsPath,
    user_id: &str,
) -> Result<HashMap<String, (i64, i64)>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series read-progress db: {error}"))?;

    let rows = sqlx::query(
        "SELECT SERIES_ID, READ_COUNT, IN_PROGRESS_COUNT \
         FROM READ_PROGRESS_SERIES \
         WHERE USER_ID = ?",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series read-progress counts: {error}"))?;

    let mut counts = HashMap::new();
    for row in rows {
        counts.insert(
            row.get::<String, _>("SERIES_ID"),
            (
                row.get::<i64, _>("READ_COUNT"),
                row.get::<i64, _>("IN_PROGRESS_COUNT"),
            ),
        );
    }
    Ok(counts)
}

pub async fn load_series_total_book_counts(
    database_file: &FsPath,
) -> Result<HashMap<String, i64>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata db: {error}"))?;

    let rows = sqlx::query(
        "SELECT SERIES_ID, TOTAL_BOOK_COUNT \
         FROM SERIES_METADATA \
         WHERE TOTAL_BOOK_COUNT IS NOT NULL",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series total-book-counts: {error}"))?;

    let mut totals = HashMap::new();
    for row in rows {
        totals.insert(
            row.get::<String, _>("SERIES_ID"),
            row.get::<i64, _>("TOTAL_BOOK_COUNT"),
        );
    }
    Ok(totals)
}
