use std::collections::HashSet;

use sqlx::{Row, SqlitePool};

use komga_application::opds::{
    BrowsePublisherEntry, BrowseSeriesNavigationEntry, BrowseSeriesNavigationPage,
    OpdsBookAuthorEntry, OpdsBookFeedEntry, OpdsBookFeedKind, OpdsBookFeedQuery,
    OpdsBrowseCatalogPort, OpdsFeedCatalogPort, OpdsFeedUserContext, OpdsLatestSeriesFeedQuery,
    OpdsLibrarySeriesQuery, OpdsPagedBooks, OpdsPagedSeries, OpdsSeriesEntry, OpdsSeriesFeedPage,
};
use komga_infrastructure_base::DatabaseHandle;
use komga_infrastructure_base::sqlite::codecs::clamp_kotlin_int_u32;

use super::records::{parsed_age_rating, parsed_book_tags, parsed_sharing_labels};

#[derive(Clone)]
pub struct OpdsCatalogAccess {
    db: DatabaseHandle,
}

impl OpdsCatalogAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl OpdsFeedCatalogPort for OpdsCatalogAccess {
    async fn load_book_feed_page(
        &self,
        query: OpdsBookFeedQuery<'_>,
    ) -> anyhow::Result<OpdsPagedBooks> {
        load_book_feed_page(self.db.read_pool(), query)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_latest_series_feed_page(
        &self,
        query: OpdsLatestSeriesFeedQuery<'_>,
    ) -> anyhow::Result<OpdsPagedSeries> {
        load_latest_series_feed_page(self.db.read_pool(), query)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_library_series_feed_page(
        &self,
        query: OpdsLibrarySeriesQuery<'_>,
    ) -> anyhow::Result<OpdsSeriesFeedPage> {
        load_library_series_feed_page(self.db.read_pool(), query)
            .await
            .map_err(anyhow::Error::from)
    }
}

#[async_trait::async_trait]
impl OpdsBrowseCatalogPort for OpdsCatalogAccess {
    async fn load_browse_series_navigation_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
        publishers: &[String],
        page: usize,
        size: usize,
    ) -> anyhow::Result<BrowseSeriesNavigationPage> {
        load_browse_series_navigation_entries(
            self.db.read_pool(),
            allowed_library_ids,
            library_id,
            publishers,
            page,
            size,
        )
        .await
        .map_err(anyhow::Error::from)
    }

    async fn load_browse_publisher_entries(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        library_id: Option<&str>,
    ) -> anyhow::Result<Vec<BrowsePublisherEntry>> {
        load_browse_publisher_entries(self.db.read_pool(), allowed_library_ids, library_id)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_series_page(
        &self,
        allowed_library_ids: Option<&HashSet<String>>,
        search: Option<&str>,
        publishers: &[String],
        offset: i64,
        limit: i64,
    ) -> anyhow::Result<Vec<OpdsSeriesEntry>> {
        load_series_page(
            self.db.read_pool(),
            allowed_library_ids,
            search,
            publishers,
            offset,
            limit,
        )
        .await
        .map_err(anyhow::Error::from)
    }
}

fn optional_non_empty_string(row: &sqlx::sqlite::SqliteRow, column: &str) -> Option<String> {
    let value = row.get::<String, _>(column);
    (!value.is_empty()).then_some(value)
}

fn parsed_book_authors(row: &sqlx::sqlite::SqliteRow) -> Vec<OpdsBookAuthorEntry> {
    row.get::<String, _>("AUTHORS")
        .split('')
        .filter(|value| !value.is_empty())
        .map(|value| {
            let mut parts = value.splitn(2, '');
            let name = parts.next().unwrap_or_default().trim().to_string();
            let role = parts.next().unwrap_or_default().trim().to_string();
            OpdsBookAuthorEntry { name, role }
        })
        .filter(|author| !author.name.is_empty())
        .collect()
}

fn sorted_authorized_library_ids(allowed_library_ids: Option<&HashSet<String>>) -> Vec<String> {
    let mut authorized_library_ids = allowed_library_ids
        .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    authorized_library_ids.sort();
    authorized_library_ids
}

fn library_visible(allowed_library_ids: Option<&HashSet<String>>, library_id: &str) -> bool {
    match allowed_library_ids {
        None => true,
        Some(ids) => ids.contains(library_id),
    }
}

async fn load_book_feed_page(
    pool: &SqlitePool,
    query: OpdsBookFeedQuery<'_>,
) -> Result<OpdsPagedBooks, sqlx::Error> {
    match query.kind {
        OpdsBookFeedKind::KeepReading => {
            let books =
                load_keep_reading_books(pool, &query.user.user_id, query.library_id).await?;
            Ok(paged_books(query.user, books, query.page, query.size))
        }
        OpdsBookFeedKind::OnDeck => {
            let books = load_on_deck_books(pool, &query.user.user_id, query.library_id).await?;
            Ok(paged_books(query.user, books, query.page, query.size))
        }
        OpdsBookFeedKind::LatestBooks {
            include_read_progress,
        } => load_latest_books_feed_page(pool, query, include_read_progress).await,
    }
}

async fn load_latest_books_feed_page(
    pool: &SqlitePool,
    query: OpdsBookFeedQuery<'_>,
    include_read_progress: bool,
) -> Result<OpdsPagedBooks, sqlx::Error> {
    let scan_limit = query.size.max(100) as i64;
    let start = query.page.saturating_mul(query.size);
    let end = start.saturating_add(query.size);
    let mut offset = 0_i64;
    let mut total_visible_books = 0_usize;
    let mut visible_page = Vec::new();

    loop {
        let batch = load_latest_books_paged(
            pool,
            query.user.allowed_library_ids(),
            include_read_progress.then_some(query.user.user_id.as_str()),
            query.library_id,
            offset,
            scan_limit,
        )
        .await?;
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        for book in batch {
            if query.user.can_access_book_feed_entry(&book) {
                if total_visible_books >= start && total_visible_books < end {
                    visible_page.push(book);
                }
                total_visible_books += 1;
            }
        }

        if batch_len < scan_limit as usize {
            break;
        }
        offset += batch_len as i64;
    }

    Ok(OpdsPagedBooks {
        books: visible_page,
        total_visible_books,
        has_next: end < total_visible_books,
    })
}

fn paged_books(
    user: &OpdsFeedUserContext,
    books: Vec<OpdsBookFeedEntry>,
    page: usize,
    size: usize,
) -> OpdsPagedBooks {
    let books = books
        .into_iter()
        .filter(|book| user.can_access_book_feed_entry(book))
        .collect::<Vec<_>>();
    let total_visible_books = books.len();
    let start = page.saturating_mul(size);
    let end = start.saturating_add(size);
    OpdsPagedBooks {
        books: books.into_iter().skip(start).take(size).collect(),
        total_visible_books,
        has_next: end < total_visible_books,
    }
}

async fn load_latest_series_feed_page(
    pool: &SqlitePool,
    query: OpdsLatestSeriesFeedQuery<'_>,
) -> Result<OpdsPagedSeries, sqlx::Error> {
    let scan_limit = query.size.max(100) as i64;
    let start = query.page.saturating_mul(query.size);
    let end = start.saturating_add(query.size);
    let mut offset = 0_i64;
    let mut total_visible_series = 0_usize;
    let mut visible_page = Vec::new();

    loop {
        let batch = load_latest_series_paged(
            pool,
            query.user.allowed_library_ids(),
            query.library_id,
            offset,
            scan_limit,
        )
        .await?;
        if batch.is_empty() {
            break;
        }

        let batch_len = batch.len();
        for series in batch {
            if query
                .user
                .can_access_series_feed_entry(&series, query.include_one_shots)
            {
                if total_visible_series >= start && total_visible_series < end {
                    visible_page.push(series);
                }
                total_visible_series += 1;
            }
        }

        if batch_len < scan_limit as usize {
            break;
        }
        offset += batch_len as i64;
    }

    Ok(OpdsPagedSeries {
        series: visible_page,
        total_visible_series,
        has_next: end < total_visible_series,
    })
}

async fn load_library_series_feed_page(
    pool: &SqlitePool,
    query: OpdsLibrarySeriesQuery<'_>,
) -> Result<OpdsSeriesFeedPage, sqlx::Error> {
    let visible_offset = query.page.saturating_mul(query.size);
    let mut raw_offset = 0_i64;
    let scan_limit = (query.size + 1).max(20) as i64;
    let mut visible_seen = 0_usize;
    let mut visible_page = Vec::with_capacity(query.size + 1);

    let has_next = loop {
        let batch = load_library_series(pool, query.library_id, raw_offset, scan_limit).await?;
        if batch.is_empty() {
            break false;
        }

        let batch_len = batch.len();
        raw_offset += batch_len as i64;
        for series in batch
            .into_iter()
            .filter(|series| query.user.can_access_series_feed_entry(series, true))
        {
            if visible_seen < visible_offset {
                visible_seen += 1;
                continue;
            }
            visible_page.push(series);
            if visible_page.len() > query.size {
                break;
            }
        }

        if visible_page.len() > query.size {
            break true;
        }
        if batch_len < scan_limit as usize {
            break false;
        }
    };

    Ok(OpdsSeriesFeedPage {
        series: visible_page.into_iter().take(query.size).collect(),
        has_next,
    })
}

async fn load_browse_series_navigation_entries(
    pool: &SqlitePool,
    allowed_library_ids: Option<&HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<BrowseSeriesNavigationPage, sqlx::Error> {
    let authorized_library_ids = sorted_authorized_library_ids(allowed_library_ids);
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(BrowseSeriesNavigationPage {
            entries: Vec::new(),
            total_count: 0,
        });
    }

    let mut clauses = vec!["s.DELETED_DATE IS NULL".to_string()];
    if library_id.is_some() {
        clauses.push("s.LIBRARY_ID = ?".to_string());
    }
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("s.LIBRARY_ID IN ({placeholders})"));
    }
    if !publishers.is_empty() {
        for _ in publishers {
            clauses.push("sm.PUBLISHER = ?".to_string());
        }
    }
    let where_clause = clauses.join(" AND ");

    let count_sql = format!(
        r#"SELECT
    COUNT(*) AS TOTAL
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
WHERE {where_clause}"#,
    );
    let mut count_query = sqlx::query(sqlx::AssertSqlSafe(count_sql));
    if let Some(id) = library_id {
        count_query = count_query.bind(id);
    }
    for library in &authorized_library_ids {
        count_query = count_query.bind(library);
    }
    for publisher in publishers {
        count_query = count_query.bind(publisher);
    }
    let total = count_query
        .fetch_one(pool)
        .await?
        .get::<i64, _>("TOTAL")
        .max(0) as usize;

    let rows_sql = format!(
        r#"SELECT
    s.ID,
    COALESCE(sm.TITLE, s.NAME) AS TITLE,
    s.LIBRARY_ID,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
WHERE {where_clause}
ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
LIMIT ?
OFFSET ?"#,
    );
    let mut rows_query = sqlx::query(sqlx::AssertSqlSafe(rows_sql));
    if let Some(id) = library_id {
        rows_query = rows_query.bind(id);
    }
    for library in &authorized_library_ids {
        rows_query = rows_query.bind(library);
    }
    for publisher in publishers {
        rows_query = rows_query.bind(publisher);
    }
    let rows = rows_query
        .bind(size as i64)
        .bind((page.saturating_mul(size)) as i64)
        .fetch_all(pool)
        .await?;

    Ok(BrowseSeriesNavigationPage {
        entries: rows
            .into_iter()
            .map(|row| BrowseSeriesNavigationEntry {
                id: row.get::<String, _>("ID"),
                title: row.get::<String, _>("TITLE"),
            })
            .collect(),
        total_count: total,
    })
}

async fn load_browse_publisher_entries(
    pool: &SqlitePool,
    allowed_library_ids: Option<&HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<BrowsePublisherEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT DISTINCT
    sm.PUBLISHER AS PUBLISHER,
    s.LIBRARY_ID AS LIBRARY_ID
FROM SERIES_METADATA sm
JOIN SERIES s ON s.ID = sm.SERIES_ID
WHERE sm.PUBLISHER IS NOT NULL
    AND trim(sm.PUBLISHER) != ''
    AND s.DELETED_DATE IS NULL
    AND (? IS NULL OR s.LIBRARY_ID = ?)
ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER"#,
    )
    .bind(library_id)
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    let mut seen = HashSet::new();
    let mut navigation = Vec::new();
    for row in rows {
        let library = row.get::<String, _>("LIBRARY_ID");
        if !library_visible(allowed_library_ids, &library) {
            continue;
        }
        let publisher = row.get::<String, _>("PUBLISHER");
        if !seen.insert(publisher.clone()) {
            continue;
        }
        navigation.push(BrowsePublisherEntry { publisher });
    }

    Ok(navigation)
}

async fn load_keep_reading_books(
    pool: &SqlitePool,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
    b.ID,
    b.LIBRARY_ID,
    b.SERIES_ID,
    COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
    COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
    COALESCE(bm.TITLE, b.NAME) AS TITLE,
    COALESCE(bm.SUMMARY, '') AS SUMMARY,
    COALESCE(bm.ISBN, '') AS ISBN,
    COALESCE(
        (SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
         FROM BOOK_METADATA_AUTHOR
         WHERE BOOK_ID = b.ID),
        ''
    ) AS AUTHORS,
    COALESCE(
        (SELECT GROUP_CONCAT(TAG, char(30))
         FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)),
        ''
    ) AS TAGS,
    COALESCE(bm.RELEASE_DATE, '') AS RELEASE_DATE,
    b.NAME AS FILE_NAME,
    COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
    COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
    COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
    COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
    rp.PAGE AS LAST_READ,
    rp.READ_DATE AS LAST_READ_DATE,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
              FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                    FROM SERIES_METADATA_SHARING sms_inner
                    WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED
FROM READ_PROGRESS rp
JOIN BOOK b ON b.ID = rp.BOOK_ID
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE rp.USER_ID = ?
    AND rp.COMPLETED = 0
    AND b.DELETED_DATE IS NULL
    AND COALESCE(m.STATUS, '') = 'READY'
    AND (? IS NULL OR b.LIBRARY_ID = ?)
GROUP BY
    b.ID,
    b.LIBRARY_ID,
    b.SERIES_ID,
    COALESCE(sm.TITLE, s.NAME),
    COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''),
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
    COALESCE(bm.TITLE, b.NAME),
    COALESCE(bm.SUMMARY, ''),
    COALESCE(bm.ISBN, ''),
    COALESCE(bm.RELEASE_DATE, ''),
    b.NAME,
    COALESCE(b.FILE_SIZE, 0),
    COALESCE(m.MEDIA_TYPE, 'application/octet-stream'),
    COALESCE(m.PAGE_COUNT, 0),
    COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
    rp.PAGE,
    rp.READ_DATE,
    COALESCE(sm.AGE_RATING, NULL),
    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '')
ORDER BY COALESCE(rp.READ_DATE, '') DESC, b.ID ASC"#,
    )
    .bind(user_id)
    .bind(library_id)
    .bind(library_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsBookFeedEntry {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: optional_non_empty_string(&row, "ISBN"),
            authors: parsed_book_authors(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            last_read: row.get::<Option<i64>, _>("LAST_READ"),
            last_read_date: row.get::<Option<String>, _>("LAST_READ_DATE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: optional_non_empty_string(&row, "RELEASE_DATE"),
        })
        .collect())
}

async fn load_on_deck_books(
    pool: &SqlitePool,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
    b.ID,
    b.LIBRARY_ID,
    b.SERIES_ID,
    COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
    COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
    COALESCE(bm.TITLE, b.NAME) AS TITLE,
    COALESCE(bm.SUMMARY, '') AS SUMMARY,
    COALESCE(bm.ISBN, '') AS ISBN,
    COALESCE(
        (SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
         FROM BOOK_METADATA_AUTHOR
         WHERE BOOK_ID = b.ID),
        ''
    ) AS AUTHORS,
    COALESCE(
        (SELECT GROUP_CONCAT(TAG, char(30))
         FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)),
        ''
    ) AS TAGS,
    COALESCE(bm.RELEASE_DATE, '') AS RELEASE_DATE,
    b.NAME AS FILE_NAME,
    COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
    COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
    COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
    COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
              FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                    FROM SERIES_METADATA_SHARING sms_inner
                    WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS ORDER_INDEX,
    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED,
    COALESCE(rps.MOST_RECENT_READ_DATE, '') AS MOST_RECENT_READ_DATE
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
LEFT JOIN READ_PROGRESS_SERIES rps ON rps.SERIES_ID = b.SERIES_ID AND rps.USER_ID = ?
WHERE b.DELETED_DATE IS NULL
    AND (? IS NULL OR b.LIBRARY_ID = ?)
    AND b.SERIES_ID IN (
        SELECT DISTINCT b_done.SERIES_ID
        FROM BOOK b_done
        JOIN READ_PROGRESS rp_done ON rp_done.BOOK_ID = b_done.ID
        WHERE rp_done.USER_ID = ?
            AND rp_done.COMPLETED = 1
    )
    AND b.SERIES_ID NOT IN (
        SELECT DISTINCT b_prog.SERIES_ID
        FROM BOOK b_prog
        JOIN READ_PROGRESS rp_prog ON rp_prog.BOOK_ID = b_prog.ID
        WHERE rp_prog.USER_ID = ?
            AND rp_prog.COMPLETED = 0
    )
    AND NOT EXISTS (
        SELECT 1
        FROM READ_PROGRESS rp_seen
        WHERE rp_seen.BOOK_ID = b.ID
            AND rp_seen.USER_ID = ?
    )
GROUP BY
    b.ID,
    b.LIBRARY_ID,
    b.SERIES_ID,
    COALESCE(sm.TITLE, s.NAME),
    COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''),
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
    COALESCE(bm.TITLE, b.NAME),
    COALESCE(bm.SUMMARY, ''),
    COALESCE(bm.RELEASE_DATE, ''),
    COALESCE(bm.ISBN, ''),
    b.NAME,
    COALESCE(b.FILE_SIZE, 0),
    COALESCE(m.MEDIA_TYPE, 'application/octet-stream'),
    COALESCE(m.PAGE_COUNT, 0),
    COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
    COALESCE(sm.AGE_RATING, NULL),
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, ''),
    COALESCE(rps.MOST_RECENT_READ_DATE, '')
ORDER BY COALESCE(rps.MOST_RECENT_READ_DATE, '') DESC, b.SERIES_ID ASC, ORDER_INDEX ASC, b.ID ASC"#,
    )
    .bind(user_id)
    .bind(library_id)
    .bind(library_id)
    .bind(user_id)
    .bind(user_id)
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    let mut seen_series = HashSet::<String>::new();
    let mut first_per_series = Vec::<OpdsBookFeedEntry>::new();
    for row in rows {
        let series_id = row.get::<String, _>("SERIES_ID");
        if !seen_series.insert(series_id) {
            continue;
        }
        first_per_series.push(OpdsBookFeedEntry {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: optional_non_empty_string(&row, "ISBN"),
            authors: parsed_book_authors(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            last_read: None,
            last_read_date: None,
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: optional_non_empty_string(&row, "RELEASE_DATE"),
        });
    }

    Ok(first_per_series)
}

async fn load_latest_books_paged(
    pool: &SqlitePool,
    allowed_library_ids: Option<&HashSet<String>>,
    user_id: Option<&str>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    let authorized_library_ids = sorted_authorized_library_ids(allowed_library_ids);
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut clauses = vec!["b.DELETED_DATE IS NULL".to_string()];
    if library_id.is_some() {
        clauses.push("b.LIBRARY_ID = ?".to_string());
    }
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("b.LIBRARY_ID IN ({placeholders})"));
    }
    let where_clause = clauses.join(" AND ");
    let sql = format!(
        r#"SELECT
    b.ID,
    b.LIBRARY_ID,
    b.SERIES_ID,
    COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
    COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER,
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT,
    COALESCE(bm.TITLE, b.NAME) AS TITLE,
    COALESCE(bm.SUMMARY, '') AS SUMMARY,
    COALESCE(bm.ISBN, '') AS ISBN,
    COALESCE(
        (SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30))
         FROM BOOK_METADATA_AUTHOR
         WHERE BOOK_ID = b.ID),
        ''
    ) AS AUTHORS,
    COALESCE(
        (SELECT GROUP_CONCAT(TAG, char(30))
         FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)),
        ''
    ) AS TAGS,
    COALESCE(bm.RELEASE_DATE, '') AS RELEASE_DATE,
    b.NAME AS FILE_NAME,
    COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE,
    COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
    COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
    COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
    rp.PAGE AS LAST_READ,
    rp.READ_DATE AS LAST_READ_DATE,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
              FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                    FROM SERIES_METADATA_SHARING sms_inner
                    WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
LEFT JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND (? IS NOT NULL AND rp.USER_ID = ?)
WHERE {where_clause}
    AND COALESCE(m.STATUS, '') = 'READY'
GROUP BY
    b.ID,
    b.LIBRARY_ID,
    b.SERIES_ID,
    COALESCE(sm.TITLE, s.NAME),
    COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''),
    COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0),
    COALESCE(bm.TITLE, b.NAME),
    COALESCE(bm.SUMMARY, ''),
    COALESCE(bm.RELEASE_DATE, ''),
    COALESCE(bm.ISBN, ''),
    b.NAME,
    COALESCE(b.FILE_SIZE, 0),
    COALESCE(m.MEDIA_TYPE, 'application/octet-stream'),
    COALESCE(m.PAGE_COUNT, 0),
    COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0),
    rp.PAGE,
    rp.READ_DATE,
    COALESCE(sm.AGE_RATING, NULL),
    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '')
ORDER BY b.CREATED_DATE DESC, b.ID DESC
LIMIT ?
OFFSET ?"#,
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    query = query.bind(user_id);
    query = query.bind(user_id);
    if let Some(id) = library_id {
        query = query.bind(id);
    }
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsBookFeedEntry {
            id: row.get::<String, _>("ID"),
            series_id: row.get::<String, _>("SERIES_ID"),
            title: row.get::<String, _>("TITLE"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            number: row.get::<String, _>("NUMBER"),
            number_sort: row.get::<f64, _>("NUMBER_SORT"),
            summary: row.get::<String, _>("SUMMARY"),
            isbn: optional_non_empty_string(&row, "ISBN"),
            authors: parsed_book_authors(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            last_read: row.get::<Option<i64>, _>("LAST_READ"),
            last_read_date: row.get::<Option<String>, _>("LAST_READ_DATE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: optional_non_empty_string(&row, "RELEASE_DATE"),
        })
        .collect())
}

async fn load_latest_series_paged(
    pool: &SqlitePool,
    allowed_library_ids: Option<&HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    let authorized_library_ids = sorted_authorized_library_ids(allowed_library_ids);
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut clauses = vec!["s.DELETED_DATE IS NULL".to_string()];
    if library_id.is_some() {
        clauses.push("s.LIBRARY_ID = ?".to_string());
    }
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("s.LIBRARY_ID IN ({placeholders})"));
    }
    let where_clause = clauses.join(" AND ");
    let sql = format!(
        r#"SELECT
    s.ID,
    s.LIBRARY_ID,
    COALESCE(sm.TITLE, s.NAME) AS TITLE,
    COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS TITLE_SORT,
    s.ONESHOT AS ONESHOT,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
              FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                    FROM SERIES_METADATA_SHARING sms_inner
                    WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE {where_clause}
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME), s.ONESHOT,
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')
ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID DESC
LIMIT ?
OFFSET ?"#,
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    if let Some(id) = library_id {
        query = query.bind(id);
    }
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsSeriesEntry {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            one_shot: row.get::<bool, _>("ONESHOT"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_library_series(
    pool: &SqlitePool,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
    s.ID,
    s.LIBRARY_ID,
    COALESCE(sm.TITLE, s.NAME) AS TITLE,
    s.ONESHOT AS ONESHOT,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
              FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                    FROM SERIES_METADATA_SHARING sms_inner
                    WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE s.DELETED_DATE IS NULL
    AND s.LIBRARY_ID = ?
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME),
         COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME), s.ONESHOT,
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')
ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
LIMIT ?
OFFSET ?"#,
    )
    .bind(library_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsSeriesEntry {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            one_shot: row.get::<bool, _>("ONESHOT"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

async fn load_series_page(
    pool: &SqlitePool,
    allowed_library_ids: Option<&HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    let authorized_library_ids = sorted_authorized_library_ids(allowed_library_ids);
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok(vec![]);
    }

    let mut clauses = vec!["s.DELETED_DATE IS NULL".to_string()];
    if !authorized_library_ids.is_empty() {
        let placeholders = (0..authorized_library_ids.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("s.LIBRARY_ID IN ({placeholders})"));
    }
    if search.is_some() {
        clauses.push("lower(COALESCE(sm.TITLE, s.NAME)) LIKE ?".to_string());
    }
    if !publishers.is_empty() {
        let placeholders = (0..publishers.len())
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        clauses.push(format!("sm.PUBLISHER IN ({placeholders})"));
    }
    let where_clause = clauses.join(" AND ");
    let sql = format!(
        r#"SELECT
    s.ID,
    s.LIBRARY_ID,
    COALESCE(sm.TITLE, s.NAME) AS TITLE,
    s.ONESHOT AS ONESHOT,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
              FROM (SELECT DISTINCT sms_inner.LABEL AS LABEL
                    FROM SERIES_METADATA_SHARING sms_inner
                    WHERE sms_inner.SERIES_ID = s.ID)), '') AS SHARING_LABELS,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE {where_clause}
GROUP BY s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME), s.ONESHOT,
         COALESCE(sm.AGE_RATING, NULL),
         COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '')
ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
LIMIT ?
OFFSET ?"#,
    );
    let mut query = sqlx::query(sqlx::AssertSqlSafe(sql));
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    if let Some(value) = search {
        query = query.bind(format!("%{}%", value.to_lowercase()));
    }
    for publisher in publishers {
        query = query.bind(publisher);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsSeriesEntry {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            one_shot: row.get::<bool, _>("ONESHOT"),
            age_rating: row
                .get::<Option<i64>, _>("AGE_RATING")
                .map(clamp_kotlin_int_u32),
            sharing_labels: row
                .get::<String, _>("SHARING_LABELS")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}
