use super::*;
use crate::discovery_persisted_access::models::{ReadProgressSummary, WebLinkEntry};

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

    Ok(map_book_summary_rows(rows))
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

    let mut rows_by_id: HashMap<String, BookSummary> = map_book_summary_rows(rows)
        .into_iter()
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

    query.build().fetch_all(pool).await
}

fn book_summary_select_sql(include_read_progress: bool) -> &'static str {
    if include_read_progress {
        r#"SELECT b.ID,
                  b.SERIES_ID,
                  b.LIBRARY_ID,
                  COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                  COALESCE(bm.TITLE, b.NAME) AS TITLE,
                  b.URL,
                  b.NUMBER,
                  b.CREATED_DATE,
                  b.LAST_MODIFIED_DATE,
                  CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  b.FILE_SIZE,
                  b.FILE_HASH,
                  s.ONESHOT AS ONESHOT,
                  b.DELETED_DATE,
                  sm.LANGUAGE AS LANGUAGE,
                  sm.PUBLISHER AS PUBLISHER,
                  sm.AGE_RATING AS AGE_RATING,
                  COALESCE((SELECT GROUP_CONCAT(DISTINCT smg.GENRE)
                            FROM SERIES_METADATA_GENRE smg
                            WHERE smg.SERIES_ID = s.ID), '') AS GENRES,
                  COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                  COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE,
                  COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                  COALESCE(m.COMMENT, '') AS MEDIA_COMMENT,
                  COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
                  COALESCE(m.EPUB_IS_KEPUB, 0) AS EPUB_IS_KEPUB,
                  COALESCE(bm.TITLE_LOCK, 0) AS METADATA_TITLE_LOCK,
                  COALESCE(bm.SUMMARY, '') AS METADATA_SUMMARY,
                  COALESCE(bm.SUMMARY_LOCK, 0) AS METADATA_SUMMARY_LOCK,
                  COALESCE(bm.NUMBER, '') AS METADATA_NUMBER,
                  COALESCE(bm.NUMBER_LOCK, 0) AS METADATA_NUMBER_LOCK,
                  COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS METADATA_NUMBER_SORT,
                  COALESCE(bm.NUMBER_SORT_LOCK, 0) AS METADATA_NUMBER_SORT_LOCK,
                  bm.RELEASE_DATE AS METADATA_RELEASE_DATE,
                  COALESCE(bm.RELEASE_DATE_LOCK, 0) AS METADATA_RELEASE_DATE_LOCK,
                  COALESCE((SELECT GROUP_CONCAT(ba.NAME || X'1E' || COALESCE(ba.ROLE, ''), X'1F')
                            FROM BOOK_METADATA_AUTHOR ba
                            WHERE ba.BOOK_ID = b.ID), '') AS METADATA_AUTHORS,
                  COALESCE(bm.AUTHORS_LOCK, 0) AS METADATA_AUTHORS_LOCK,
                  COALESCE((SELECT GROUP_CONCAT(bt.TAG)
                            FROM BOOK_METADATA_TAG bt
                            WHERE bt.BOOK_ID = b.ID), '') AS METADATA_TAGS,
                  COALESCE(bm.TAGS_LOCK, 0) AS METADATA_TAGS_LOCK,
                  COALESCE(bm.ISBN, '') AS METADATA_ISBN,
                  COALESCE(bm.ISBN_LOCK, 0) AS METADATA_ISBN_LOCK,
                  COALESCE((SELECT GROUP_CONCAT(bl.LABEL || X'1E' || bl.URL, X'1F')
                            FROM BOOK_METADATA_LINK bl
                            WHERE bl.BOOK_ID = b.ID), '') AS METADATA_LINKS,
                  COALESCE(bm.LINKS_LOCK, 0) AS METADATA_LINKS_LOCK,
                  COALESCE(bm.CREATED_DATE, b.CREATED_DATE) AS METADATA_CREATED,
                  COALESCE(bm.LAST_MODIFIED_DATE, b.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                  CASE
                    WHEN rp.BOOK_ID IS NULL THEN 'unread'
                    WHEN rp.COMPLETED = 1 THEN 'read'
                    ELSE 'in_progress'
                  END AS READ_STATUS,
                  rp.PAGE AS READ_PROGRESS_PAGE,
                  rp.COMPLETED AS READ_PROGRESS_COMPLETED,
                  rp.READ_DATE AS READ_PROGRESS_READ_DATE,
                  rp.CREATED_DATE AS READ_PROGRESS_CREATED,
                  rp.LAST_MODIFIED_DATE AS READ_PROGRESS_LAST_MODIFIED,
                  rp.DEVICE_ID AS READ_PROGRESS_DEVICE_ID,
                  rp.DEVICE_NAME AS READ_PROGRESS_DEVICE_NAME
           FROM BOOK b
           JOIN SERIES s ON s.ID = b.SERIES_ID
           LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
           LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
           LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
            LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID
                                   AND rp.USER_ID = "#
    } else {
        r#"SELECT b.ID,
                  b.SERIES_ID,
                  b.LIBRARY_ID,
                  COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                  COALESCE(bm.TITLE, b.NAME) AS TITLE,
                  b.URL,
                  b.NUMBER,
                  b.CREATED_DATE,
                  b.LAST_MODIFIED_DATE,
                  CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                  b.FILE_SIZE,
                  b.FILE_HASH,
                  s.ONESHOT AS ONESHOT,
                  b.DELETED_DATE,
                  sm.LANGUAGE AS LANGUAGE,
                  sm.PUBLISHER AS PUBLISHER,
                  sm.AGE_RATING AS AGE_RATING,
                  COALESCE((SELECT GROUP_CONCAT(DISTINCT smg.GENRE)
                            FROM SERIES_METADATA_GENRE smg
                            WHERE smg.SERIES_ID = s.ID), '') AS GENRES,
                  COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                  COALESCE(m.MEDIA_TYPE, '') AS MEDIA_TYPE,
                  COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT,
                  COALESCE(m.COMMENT, '') AS MEDIA_COMMENT,
                  COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
                  COALESCE(m.EPUB_IS_KEPUB, 0) AS EPUB_IS_KEPUB,
                  COALESCE(bm.TITLE_LOCK, 0) AS METADATA_TITLE_LOCK,
                  COALESCE(bm.SUMMARY, '') AS METADATA_SUMMARY,
                  COALESCE(bm.SUMMARY_LOCK, 0) AS METADATA_SUMMARY_LOCK,
                  COALESCE(bm.NUMBER, '') AS METADATA_NUMBER,
                  COALESCE(bm.NUMBER_LOCK, 0) AS METADATA_NUMBER_LOCK,
                  COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS METADATA_NUMBER_SORT,
                  COALESCE(bm.NUMBER_SORT_LOCK, 0) AS METADATA_NUMBER_SORT_LOCK,
                  bm.RELEASE_DATE AS METADATA_RELEASE_DATE,
                  COALESCE(bm.RELEASE_DATE_LOCK, 0) AS METADATA_RELEASE_DATE_LOCK,
                  COALESCE((SELECT GROUP_CONCAT(ba.NAME || X'1E' || COALESCE(ba.ROLE, ''), X'1F')
                            FROM BOOK_METADATA_AUTHOR ba
                            WHERE ba.BOOK_ID = b.ID), '') AS METADATA_AUTHORS,
                  COALESCE(bm.AUTHORS_LOCK, 0) AS METADATA_AUTHORS_LOCK,
                  COALESCE((SELECT GROUP_CONCAT(bt.TAG)
                            FROM BOOK_METADATA_TAG bt
                            WHERE bt.BOOK_ID = b.ID), '') AS METADATA_TAGS,
                  COALESCE(bm.TAGS_LOCK, 0) AS METADATA_TAGS_LOCK,
                  COALESCE(bm.ISBN, '') AS METADATA_ISBN,
                  COALESCE(bm.ISBN_LOCK, 0) AS METADATA_ISBN_LOCK,
                  COALESCE((SELECT GROUP_CONCAT(bl.LABEL || X'1E' || bl.URL, X'1F')
                            FROM BOOK_METADATA_LINK bl
                            WHERE bl.BOOK_ID = b.ID), '') AS METADATA_LINKS,
                  COALESCE(bm.LINKS_LOCK, 0) AS METADATA_LINKS_LOCK,
                  COALESCE(bm.CREATED_DATE, b.CREATED_DATE) AS METADATA_CREATED,
                  COALESCE(bm.LAST_MODIFIED_DATE, b.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                  'unread' AS READ_STATUS,
                  NULL AS READ_PROGRESS_PAGE,
                  NULL AS READ_PROGRESS_COMPLETED,
                  NULL AS READ_PROGRESS_READ_DATE,
                  NULL AS READ_PROGRESS_CREATED,
                  NULL AS READ_PROGRESS_LAST_MODIFIED,
                  NULL AS READ_PROGRESS_DEVICE_ID,
                  NULL AS READ_PROGRESS_DEVICE_NAME
           FROM BOOK b
           JOIN SERIES s ON s.ID = b.SERIES_ID
           LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
           LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
            LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID"#
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
        series_title: row.get::<String, _>("SERIES_TITLE"),
        title: row.get::<String, _>("TITLE"),
        url: row.get::<String, _>("URL"),
        number: row.get::<i64, _>("NUMBER") as i32,
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        size_bytes: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        media_status: row.get::<String, _>("MEDIA_STATUS"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        media_pages_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u32,
        media_comment: row.get::<String, _>("MEDIA_COMMENT"),
        media_epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
        media_epub_is_kepub: row.get::<bool, _>("EPUB_IS_KEPUB"),
        read_status: row.get::<String, _>("READ_STATUS"),
        metadata_title_lock: row.get::<bool, _>("METADATA_TITLE_LOCK"),
        metadata_summary: row.get::<String, _>("METADATA_SUMMARY"),
        metadata_summary_lock: row.get::<bool, _>("METADATA_SUMMARY_LOCK"),
        metadata_number: row.get::<String, _>("METADATA_NUMBER"),
        metadata_number_lock: row.get::<bool, _>("METADATA_NUMBER_LOCK"),
        metadata_number_sort: row.get::<f64, _>("METADATA_NUMBER_SORT"),
        metadata_number_sort_lock: row.get::<bool, _>("METADATA_NUMBER_SORT_LOCK"),
        metadata_release_date: row.get::<Option<String>, _>("METADATA_RELEASE_DATE"),
        metadata_release_date_lock: row.get::<bool, _>("METADATA_RELEASE_DATE_LOCK"),
        metadata_authors_lock: row.get::<bool, _>("METADATA_AUTHORS_LOCK"),
        metadata_tags_lock: row.get::<bool, _>("METADATA_TAGS_LOCK"),
        metadata_isbn: row.get::<String, _>("METADATA_ISBN"),
        metadata_isbn_lock: row.get::<bool, _>("METADATA_ISBN_LOCK"),
        metadata_links_lock: row.get::<bool, _>("METADATA_LINKS_LOCK"),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        file_hash: row.get::<String, _>("FILE_HASH"),
        read_progress: parse_read_progress_summary(&row),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<bool, _>("ONESHOT"),
        genres: common::parse_csv_values(&row.get::<String, _>("GENRES")),
        language: row.get::<Option<String>, _>("LANGUAGE"),
        publisher: row.get::<Option<String>, _>("PUBLISHER"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value.max(0) as u16),
        metadata_tags: common::parse_csv_values(&row.get::<String, _>("METADATA_TAGS")),
        metadata_authors: parse_author_entries(&row.get::<String, _>("METADATA_AUTHORS")),
        metadata_links: parse_web_link_entries(&row.get::<String, _>("METADATA_LINKS")),
    }
}

fn map_book_summary_rows(rows: Vec<sqlx::sqlite::SqliteRow>) -> Vec<BookSummary> {
    rows.into_iter().map(map_book_summary).collect()
}

fn parse_read_progress_summary(row: &sqlx::sqlite::SqliteRow) -> Option<ReadProgressSummary> {
    row.get::<Option<i64>, _>("READ_PROGRESS_PAGE")
        .map(|page| ReadProgressSummary {
            page: page as i32,
            completed: row
                .get::<Option<bool>, _>("READ_PROGRESS_COMPLETED")
                .unwrap_or(false),
            read_date: row.get::<Option<String>, _>("READ_PROGRESS_READ_DATE"),
            created: row
                .get::<Option<String>, _>("READ_PROGRESS_CREATED")
                .unwrap_or_default(),
            last_modified: row
                .get::<Option<String>, _>("READ_PROGRESS_LAST_MODIFIED")
                .unwrap_or_default(),
            device_id: row
                .get::<Option<String>, _>("READ_PROGRESS_DEVICE_ID")
                .unwrap_or_default(),
            device_name: row
                .get::<Option<String>, _>("READ_PROGRESS_DEVICE_NAME")
                .unwrap_or_default(),
        })
}

fn split_non_empty_entries(raw: &str) -> impl Iterator<Item = &str> + '_ {
    raw.split('\u{001F}').filter(|entry| !entry.is_empty())
}

fn parse_author_entries(raw: &str) -> Vec<AuthorEntry> {
    split_non_empty_entries(raw)
        .map(|entry| match entry.split_once('\u{001E}') {
            Some((name, role)) => AuthorEntry {
                name: name.to_string(),
                role: role.to_string(),
            },
            None => AuthorEntry {
                name: entry.to_string(),
                role: String::new(),
            },
        })
        .collect()
}

fn parse_web_link_entries(raw: &str) -> Vec<WebLinkEntry> {
    split_non_empty_entries(raw)
        .filter_map(|entry| {
            entry
                .split_once('\u{001E}')
                .map(|(label, url)| WebLinkEntry {
                    label: label.to_string(),
                    url: url.to_string(),
                })
        })
        .collect()
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
