use std::collections::{BTreeSet, HashMap};

use komga_application::discovery::SeriesReadProgressCounts;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::models::{BookBrowseEntry, BookTagsScope};

pub(super) async fn load_persisted_ondeck_books(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<BookBrowseEntry>, String> {
    let rows = sqlx::query(
        r#"SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE
         FROM READ_PROGRESS_SERIES rps
         JOIN SERIES s ON s.ID = rps.SERIES_ID
         JOIN BOOK b ON b.SERIES_ID = s.ID
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = rps.USER_ID
         WHERE rps.USER_ID = ?
         AND rps.IN_PROGRESS_COUNT = 0
         AND rps.READ_COUNT != s.BOOK_COUNT
         AND rp.COMPLETED IS NULL
         AND NOT EXISTS (SELECT 1
                         FROM BOOK b_prev
                         JOIN BOOK_METADATA bm_prev ON bm_prev.BOOK_ID = b_prev.ID
                         LEFT JOIN READ_PROGRESS rp_prev ON rp_prev.BOOK_ID = b_prev.ID
                                                      AND rp_prev.USER_ID = rps.USER_ID
                         WHERE b_prev.SERIES_ID = b.SERIES_ID
                         AND rp_prev.COMPLETED IS NULL
                         AND (COALESCE(bm_prev.NUMBER_SORT, 0) < COALESCE(bm.NUMBER_SORT, 0)
                              OR (COALESCE(bm_prev.NUMBER_SORT, 0) = COALESCE(bm.NUMBER_SORT, 0)
                                  AND b_prev.ID < b.ID)))
         ORDER BY rps.MOST_RECENT_READ_DATE DESC"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted books ondeck: {error}"))?;

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

pub(super) async fn load_persisted_duplicate_books(
    pool: &SqlitePool,
) -> Result<Vec<BookBrowseEntry>, String> {
    let rows = sqlx::query(
        r#"SELECT b.ID, b.LIBRARY_ID, b.NAME, COALESCE(bm.TITLE, b.NAME) AS TITLE
         FROM BOOK b
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.FILE_HASH IS NOT NULL
         AND b.FILE_HASH != ''
         AND b.FILE_HASH IN (SELECT FILE_HASH
                            FROM BOOK
                            WHERE FILE_HASH IS NOT NULL
                            AND FILE_HASH != ''
                            GROUP BY FILE_HASH, FILE_SIZE
                            HAVING COUNT(*) > 1)"#,
    )
    .fetch_all(pool)
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

pub(super) async fn load_persisted_book_tags(
    pool: &SqlitePool,
    scope: Option<&BookTagsScope>,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    let Some(scope) = scope else {
        return Ok(vec![]);
    };

    if let Some(authorized_library_ids) = authorized_library_ids
        && authorized_library_ids.is_empty()
    {
        return Ok(vec![]);
    }

    let rows = match scope {
        BookTagsScope::All => {
            let mut query = QueryBuilder::<Sqlite>::new(
                r#"SELECT bt.TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID"#,
            );
            if let Some(authorized_library_ids) =
                authorized_library_ids.filter(|ids| !ids.is_empty())
            {
                query.push(r#" WHERE b.LIBRARY_ID IN ("#);
                let mut separated = query.separated(",");
                for library_id in authorized_library_ids {
                    separated.push_bind(library_id);
                }
                separated.push_unseparated(")");
            }
            query.push(r#" ORDER BY lower(bt.TAG), bt.TAG, b.ID"#);
            query.build().fetch_all(pool).await
        }
        BookTagsScope::Series(series_id) => {
            let mut query = QueryBuilder::<Sqlite>::new(
                r#"SELECT bt.TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
                 WHERE b.SERIES_ID = "#,
            );
            query.push_bind(series_id);
            if let Some(authorized_library_ids) =
                authorized_library_ids.filter(|ids| !ids.is_empty())
            {
                query.push(r#" AND b.LIBRARY_ID IN ("#);
                let mut separated = query.separated(",");
                for library_id in authorized_library_ids {
                    separated.push_bind(library_id);
                }
                separated.push_unseparated(")");
            }
            query.push(r#" ORDER BY lower(bt.TAG), bt.TAG, b.ID"#);
            query.build().fetch_all(pool).await
        }
        BookTagsScope::Libraries(library_ids) => {
            let mut query = QueryBuilder::<Sqlite>::new(
                r#"SELECT bt.TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
                 WHERE b.LIBRARY_ID IN ("#,
            );
            let mut separated = query.separated(",");
            for library_id in library_ids {
                separated.push_bind(library_id);
            }
            separated.push_unseparated(")");
            if let Some(authorized_library_ids) =
                authorized_library_ids.filter(|ids| !ids.is_empty())
            {
                query.push(r#" AND b.LIBRARY_ID IN ("#);
                let mut separated = query.separated(",");
                for library_id in authorized_library_ids {
                    separated.push_bind(library_id);
                }
                separated.push_unseparated(")");
            }
            query.push(r#" ORDER BY lower(bt.TAG), bt.TAG, b.ID"#);
            query.build().fetch_all(pool).await
        }
        BookTagsScope::ReadList(readlist_id) => {
            let mut query = QueryBuilder::<Sqlite>::new(
                r#"SELECT bt.TAG
                 FROM BOOK_METADATA_TAG bt
                 JOIN BOOK b ON b.ID = bt.BOOK_ID
                 JOIN READLIST_BOOK rb ON rb.BOOK_ID = b.ID
                 WHERE rb.READLIST_ID = "#,
            );
            query.push_bind(readlist_id);
            if let Some(authorized_library_ids) =
                authorized_library_ids.filter(|ids| !ids.is_empty())
            {
                query.push(r#" AND b.LIBRARY_ID IN ("#);
                let mut separated = query.separated(",");
                for library_id in authorized_library_ids {
                    separated.push_bind(library_id);
                }
                separated.push_unseparated(")");
            }
            query.push(r#" ORDER BY lower(bt.TAG), bt.TAG, b.ID"#);
            query.build().fetch_all(pool).await
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

pub(crate) async fn persisted_utc_date_minus_days(
    pool: &SqlitePool,
    days: i64,
) -> Result<Option<String>, String> {
    let modifier = if days >= 0 {
        format!("-{days} days")
    } else {
        format!("+{} days", days.saturating_abs())
    };

    let row = sqlx::query(r#"SELECT date('now', ?) AS CUTOFF"#)
        .bind(modifier)
        .fetch_one(pool)
        .await
        .map_err(|error| format!("query persisted utc cutoff date: {error}"))?;

    Ok(row.get::<Option<String>, _>("CUTOFF"))
}

pub(crate) async fn load_series_read_progress_counts(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<HashMap<String, SeriesReadProgressCounts>, String> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID, READ_COUNT, IN_PROGRESS_COUNT
         FROM READ_PROGRESS_SERIES
         WHERE USER_ID = ?"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query series read-progress counts: {error}"))?;

    let mut counts = HashMap::new();
    for row in rows {
        counts.insert(
            row.get::<String, _>("SERIES_ID"),
            SeriesReadProgressCounts {
                read_count: row.get::<i64, _>("READ_COUNT"),
                in_progress_count: row.get::<i64, _>("IN_PROGRESS_COUNT"),
            },
        );
    }
    Ok(counts)
}

pub(crate) async fn load_series_read_dates(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<HashMap<String, String>, String> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID, MOST_RECENT_READ_DATE
         FROM READ_PROGRESS_SERIES
         WHERE USER_ID = ?
           AND MOST_RECENT_READ_DATE IS NOT NULL"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query series read dates: {error}"))?;

    let mut dates = HashMap::new();
    for row in rows {
        dates.insert(
            row.get::<String, _>("SERIES_ID"),
            row.get::<String, _>("MOST_RECENT_READ_DATE"),
        );
    }

    Ok(dates)
}

pub(crate) async fn load_series_total_book_counts(
    pool: &SqlitePool,
) -> Result<HashMap<String, i64>, String> {
    let rows = sqlx::query(
        r#"SELECT SERIES_ID, TOTAL_BOOK_COUNT
         FROM SERIES_METADATA
         WHERE TOTAL_BOOK_COUNT IS NOT NULL"#,
    )
    .fetch_all(pool)
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
