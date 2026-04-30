use std::collections::HashSet;
use std::path::Path;

use crate::sqlite::connect_read_pool;
use sqlx::Row;

pub struct BrowseSeriesNavigationEntry {
    pub id: String,
    pub title: String,
}

pub struct BrowsePublisherEntry {
    pub publisher: String,
}

pub struct OpdsBookAuthorEntry {
    pub name: String,
    pub role: String,
}

pub struct OpdsBookFeedEntry {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<OpdsBookAuthorEntry>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub last_read: Option<i64>,
    pub last_read_date: Option<String>,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct OpdsSeriesEntry {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub one_shot: bool,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

fn parsed_age_rating(row: &sqlx::sqlite::SqliteRow) -> Option<u16> {
    row.try_get::<i64, _>("AGE_RATING")
        .ok()
        .and_then(|value| u16::try_from(value).ok())
}

fn parsed_sharing_labels(row: &sqlx::sqlite::SqliteRow) -> Vec<String> {
    row.get::<String, _>("SHARING_LABELS")
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
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

fn parsed_book_tags(row: &sqlx::sqlite::SqliteRow) -> Vec<String> {
    row.get::<String, _>("TAGS")
        .split('')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub struct OpdsReadlistEntry {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

fn sorted_authorized_library_ids(allowed_library_ids: &Option<HashSet<String>>) -> Vec<String> {
    let mut authorized_library_ids = allowed_library_ids
        .as_ref()
        .map(|ids| ids.iter().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    authorized_library_ids.sort();
    authorized_library_ids
}

fn library_visible(allowed_library_ids: &Option<HashSet<String>>, library_id: &str) -> bool {
    match allowed_library_ids {
        None => true,
        Some(ids) => ids.contains(library_id),
    }
}

pub async fn load_browse_series_navigation_entries(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<(Vec<BrowseSeriesNavigationEntry>, usize), sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let authorized_library_ids = sorted_authorized_library_ids(allowed_library_ids);
    if allowed_library_ids.is_some() && authorized_library_ids.is_empty() {
        return Ok((vec![], 0));
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
    let mut count_query = sqlx::query(count_sql.as_str());
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
        .fetch_one(&pool)
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
    let mut rows_query = sqlx::query(rows_sql.as_str());
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
        .fetch_all(&pool)
        .await?;

    Ok((
        rows.into_iter()
            .map(|row| BrowseSeriesNavigationEntry {
                id: row.get::<String, _>("ID"),
                title: row.get::<String, _>("TITLE"),
            })
            .collect(),
        total,
    ))
}

pub async fn load_browse_publisher_entries(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<BrowsePublisherEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
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
    .fetch_all(&pool)
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

pub async fn load_keep_reading_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
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
    COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
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
    .fetch_all(&pool)
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
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
            authors: parsed_book_authors(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            last_read: row.try_get::<Option<i64>, _>("LAST_READ").ok().flatten(),
            last_read_date: row
                .try_get::<Option<String>, _>("LAST_READ_DATE")
                .ok()
                .flatten(),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row
                .try_get::<String, _>("RELEASE_DATE")
                .ok()
                .filter(|value| !value.is_empty()),
        })
        .collect())
}

pub async fn load_on_deck_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
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
    COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
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
    .fetch_all(&pool)
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
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
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
            release_date: row
                .try_get::<String, _>("RELEASE_DATE")
                .ok()
                .filter(|value| !value.is_empty()),
        });
    }

    Ok(first_per_series)
}

pub async fn load_latest_books(
    database_file: &Path,
    library_id: Option<&str>,
    limit: i64,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    load_latest_books_paged(database_file, &None, None, library_id, 0, limit).await
}

pub async fn load_latest_books_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    user_id: Option<&str>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsBookFeedEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
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
    COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
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
    let mut query = sqlx::query(sql.as_str());
    query = query.bind(user_id);
    query = query.bind(user_id);
    if let Some(id) = library_id {
        query = query.bind(id);
    }
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(&pool).await?;

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
            isbn: row
                .try_get::<String, _>("ISBN")
                .ok()
                .filter(|value| !value.is_empty()),
            authors: parsed_book_authors(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            last_read: row.try_get::<Option<i64>, _>("LAST_READ").ok().flatten(),
            last_read_date: row
                .try_get::<Option<String>, _>("LAST_READ_DATE")
                .ok()
                .flatten(),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row
                .try_get::<String, _>("RELEASE_DATE")
                .ok()
                .filter(|value| !value.is_empty()),
        })
        .collect())
}

pub async fn load_latest_series(
    database_file: &Path,
    library_id: Option<&str>,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    load_latest_series_paged(database_file, &None, library_id, 0, limit).await
}

pub async fn load_latest_series_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
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
    COALESCE(s.ONESHOT, 0) AS ONESHOT,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE {where_clause}
GROUP BY s.ID, s.LIBRARY_ID, TITLE, ONESHOT, AGE_RATING, LAST_MODIFIED
ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID DESC
LIMIT ?
OFFSET ?"#,
    );
    let mut query = sqlx::query(sql.as_str());
    if let Some(id) = library_id {
        query = query.bind(id);
    }
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(&pool).await?;

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

pub async fn load_library_series(
    database_file: &Path,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let rows = sqlx::query(
        r#"SELECT
    s.ID,
    s.LIBRARY_ID,
    COALESCE(sm.TITLE, s.NAME) AS TITLE,
    COALESCE(s.ONESHOT, 0) AS ONESHOT,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE s.DELETED_DATE IS NULL
    AND s.LIBRARY_ID = ?
GROUP BY s.ID, s.LIBRARY_ID, TITLE, TITLE_SORT, ONESHOT, AGE_RATING, LAST_MODIFIED
ORDER BY TITLE_SORT COLLATE NOCASE ASC, s.ID ASC
LIMIT ?
OFFSET ?"#,
    )
    .bind(library_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
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

pub async fn load_series_page(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<OpdsSeriesEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
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
    COALESCE(s.ONESHOT, 0) AS ONESHOT,
    COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING,
    COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS,
    COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED
FROM SERIES s
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
WHERE {where_clause}
GROUP BY s.ID, s.LIBRARY_ID, TITLE, ONESHOT, AGE_RATING, LAST_MODIFIED
ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
LIMIT ?
OFFSET ?"#,
    );
    let mut query = sqlx::query(sql.as_str());
    for id in &authorized_library_ids {
        query = query.bind(id);
    }
    if let Some(value) = search {
        query = query.bind(format!("%{}%", value.to_lowercase()));
    }
    for publisher in publishers {
        query = query.bind(publisher);
    }
    let rows = query.bind(limit).bind(offset).fetch_all(&pool).await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsSeriesEntry {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
            one_shot: row.get::<bool, _>("ONESHOT"),
            age_rating: row
                .try_get::<i64, _>("AGE_RATING")
                .ok()
                .and_then(|value| u16::try_from(value).ok()),
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

pub async fn load_all_readlists(
    database_file: &Path,
) -> Result<Vec<OpdsReadlistEntry>, sqlx::Error> {
    let pool = connect_read_pool(database_file).await?;
    let rows = sqlx::query(
        r#"SELECT
    ID,
    NAME,
    COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED
FROM READLIST
ORDER BY NAME COLLATE NOCASE ASC, ID ASC"#,
    )
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| OpdsReadlistEntry {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}
