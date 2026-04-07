use std::collections::HashSet;
use std::path::Path;

use crate::sqlite::connect_pool;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use sqlx::Row;
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

#[derive(Clone)]
pub struct PersistedLibraryRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
}

pub struct PersistedSeriesRecord {
    pub id: String,
    pub library_id: String,
    pub title: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
}

pub struct PersistedSeriesBookRecord {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub authors: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub last_read: Option<i64>,
    pub last_read_date: Option<String>,
    pub last_modified: String,
}

pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
    pub ordered: bool,
}

pub struct PersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}

pub struct PersistedReadlistBookRecord {
    pub id: String,
    pub series_id: String,
    pub title: String,
    pub series_title: String,
    pub number: String,
    pub number_sort: f64,
    pub summary: String,
    pub isbn: Option<String>,
    pub authors: Vec<PersistedBookAuthorRecord>,
    pub tags: Vec<String>,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
    pub media_status: Option<String>,
    pub page_count: i64,
    pub epub_divina_compatible: bool,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
    pub last_modified: String,
    pub release_date: Option<String>,
}

pub struct PersistedSeriesSearchRecord {
    pub id: String,
    pub title: String,
    pub library_id: String,
    pub last_modified: String,
}

pub struct PersistedBookSearchRecord {
    pub id: String,
    pub title: String,
    pub library_id: String,
}

pub struct PersistedNamedRecord {
    pub id: String,
    pub name: String,
    pub last_modified: String,
    pub ordered: bool,
}

pub struct PersistedBookFeedRecord {
    pub id: String,
    pub title: String,
    pub file_name: String,
    pub media_type: String,
    pub library_id: String,
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

fn parsed_book_author_records(row: &sqlx::sqlite::SqliteRow) -> Vec<PersistedBookAuthorRecord> {
    row.get::<String, _>("AUTHORS")
        .split('\u{001e}')
        .filter(|value| !value.is_empty())
        .map(|value| {
            let mut parts = value.splitn(2, '\u{001f}');
            let name = parts.next().unwrap_or_default().trim().to_string();
            let role = parts.next().unwrap_or_default().trim().to_string();
            PersistedBookAuthorRecord { name, role }
        })
        .filter(|author| !author.name.is_empty())
        .collect()
}

fn parsed_book_tags(row: &sqlx::sqlite::SqliteRow) -> Vec<String> {
    row.get::<String, _>("TAGS")
        .split('\u{001e}')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn placeholder_list(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn load_libraries(
    database_file: &Path,
) -> Result<Vec<PersistedLibraryRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM LIBRARY",
    )
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedLibraryRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_library(
    database_file: &Path,
    library_id: &str,
) -> Result<Option<PersistedLibraryRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM LIBRARY \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(library_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedLibraryRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

pub async fn load_readlists_for_library(
    database_file: &Path,
    library_id: &str,
) -> Result<Vec<PersistedReadlistRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT DISTINCT rl.ID, rl.NAME, rl.ORDERED, \
                COALESCE(rl.LAST_MODIFIED_DATE, rl.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READLIST rl \
         JOIN READLIST_BOOK rb ON rb.READLIST_ID = rl.ID \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         WHERE b.LIBRARY_ID = ? \
         ORDER BY rl.NAME COLLATE NOCASE ASC, rl.ID ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            ordered: row.get::<bool, _>("ORDERED"),
        })
        .collect())
}

pub async fn load_series(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<PersistedSeriesRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE s.ID = ? \
         GROUP BY s.ID, s.LIBRARY_ID, TITLE, AGE_RATING, LAST_MODIFIED \
         LIMIT 1",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedSeriesRecord {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
        age_rating: parsed_age_rating(&row),
        sharing_labels: parsed_sharing_labels(&row),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
    }))
}

pub async fn load_series_books_paged(
    database_file: &Path,
    series_id: &str,
    user_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeriesBookRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, \
                COALESCE(bm.SUMMARY, '') AS SUMMARY, \
                COALESCE(GROUP_CONCAT(DISTINCT bma.NAME), '') AS AUTHORS, \
                b.NAME AS FILE_NAME, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, \
                COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE, \
                rp.PAGE AS LAST_READ, \
                rp.READ_DATE AS LAST_READ_DATE, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
          FROM BOOK b \
          LEFT \
          JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
          LEFT \
          JOIN BOOK_METADATA_AUTHOR bma ON bma.BOOK_ID = b.ID \
          LEFT \
          JOIN MEDIA m ON m.BOOK_ID = b.ID \
          LEFT \
          JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ? \
          WHERE b.SERIES_ID = ? \
          AND b.DELETED_DATE IS NULL \
          AND COALESCE(m.STATUS, '') = 'READY' \
          GROUP BY b.ID, COALESCE(bm.TITLE, b.NAME), COALESCE(bm.SUMMARY, ''), \
                   b.NAME, COALESCE(b.FILE_SIZE, 0), \
                   COALESCE(m.MEDIA_TYPE, 'application/octet-stream'), \
                   COALESCE(m.PAGE_COUNT, 0), \
                   COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0), \
                   rp.PAGE, rp.READ_DATE, \
                   COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') \
           ORDER BY COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC, b.ID ASC \
           LIMIT ? \
           OFFSET ?",
    )
    .bind(user_id)
    .bind(series_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesBookRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            summary: row.get::<String, _>("SUMMARY"),
            authors: row
                .get::<String, _>("AUTHORS")
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect(),
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
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_readlist(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Option<PersistedReadlistRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM READLIST \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedReadlistRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row.get::<String, _>("LAST_MODIFIED"),
        ordered: row.get::<bool, _>("ORDERED"),
    }))
}

pub async fn load_readlist_books(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBookRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, \
                COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE, \
                COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), '') AS NUMBER, \
                COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) AS NUMBER_SORT, \
                COALESCE(bm.SUMMARY, '') AS SUMMARY, \
                COALESCE(bm.ISBN, '') AS ISBN, \
                COALESCE((SELECT GROUP_CONCAT(NAME || char(31) || COALESCE(ROLE, ''), char(30)) \
                          FROM BOOK_METADATA_AUTHOR \
                          WHERE BOOK_ID = b.ID), '') AS AUTHORS, \
                COALESCE((SELECT GROUP_CONCAT(TAG, char(30)) \
                          FROM (SELECT DISTINCT TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = b.ID)), '') AS TAGS, \
                b.NAME AS FILE_NAME, COALESCE(b.FILE_SIZE, 0) AS FILE_SIZE, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                m.STATUS AS MEDIA_STATUS, \
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, \
                COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED, \
                bm.RELEASE_DATE AS RELEASE_DATE \
         FROM READLIST_BOOK rb \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         LEFT \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
           WHERE rb.READLIST_ID = ? \
           AND b.DELETED_DATE IS NULL \
          GROUP BY b.ID, b.SERIES_ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME), \
                   COALESCE(sm.TITLE, s.NAME), COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT), ''), \
                   COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0), COALESCE(bm.SUMMARY, ''), COALESCE(bm.ISBN, ''), \
                   b.NAME, COALESCE(b.FILE_SIZE, 0), \
                    COALESCE(m.MEDIA_TYPE, 'application/octet-stream'), m.STATUS, \
                    COALESCE(m.PAGE_COUNT, 0), COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0), \
                    COALESCE(sm.AGE_RATING, NULL), \
                    COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, ''), \
                    bm.RELEASE_DATE \
          ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistBookRecord {
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
            authors: parsed_book_author_records(&row),
            tags: parsed_book_tags(&row),
            file_name: row.get::<String, _>("FILE_NAME"),
            file_size: row.get::<i64, _>("FILE_SIZE"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            media_status: row.try_get::<String, _>("MEDIA_STATUS").ok(),
            page_count: row.get::<i64, _>("PAGE_COUNT"),
            epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
            release_date: row.try_get::<String, _>("RELEASE_DATE").ok(),
        })
        .collect())
}

pub async fn load_series_search_count(database_file: &Path) -> Result<usize, sqlx::Error> {
    if !database_file.exists() {
        return Ok(0);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COUNT(*) AS TOTAL \
         FROM SERIES s \
         WHERE s.DELETED_DATE IS NULL",
    )
    .fetch_one(&pool)
    .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_book_search_count(database_file: &Path) -> Result<usize, sqlx::Error> {
    if !database_file.exists() {
        return Ok(0);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT COUNT(*) AS TOTAL \
         FROM BOOK b \
         WHERE b.DELETED_DATE IS NULL",
    )
    .fetch_one(&pool)
    .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_collection_search_count(database_file: &Path) -> Result<usize, sqlx::Error> {
    if !database_file.exists() {
        return Ok(0);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query("SELECT COUNT(*) AS TOTAL FROM COLLECTION")
        .fetch_one(&pool)
        .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_readlist_search_count(database_file: &Path) -> Result<usize, sqlx::Error> {
    if !database_file.exists() {
        return Ok(0);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query("SELECT COUNT(*) AS TOTAL FROM READLIST")
        .fetch_one(&pool)
        .await?;
    Ok(row.get::<i64, _>("TOTAL") as usize)
}

pub async fn load_series_search_records_by_ids(
    database_file: &Path,
    ids: &[String],
) -> Result<Vec<PersistedSeriesSearchRecord>, sqlx::Error> {
    if !database_file.exists() || ids.is_empty() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let sql = format!(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
          FROM SERIES s \
          LEFT \
          JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.DELETED_DATE IS NULL \
         AND s.ID IN ({})",
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(&pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesSearchRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_book_search_records_by_ids(
    database_file: &Path,
    ids: &[String],
) -> Result<Vec<PersistedBookSearchRecord>, sqlx::Error> {
    if !database_file.exists() || ids.is_empty() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let sql = format!(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         AND b.ID IN ({})",
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(&pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedBookSearchRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
        })
        .collect())
}

pub async fn load_collection_search_records_by_ids(
    database_file: &Path,
    ids: &[String],
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if !database_file.exists() || ids.is_empty() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let sql = format!(
        "SELECT ID, NAME \
         FROM COLLECTION \
         WHERE ID IN ({})",
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }

    Ok(query
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: false,
        })
        .collect())
}

pub async fn load_readlist_search_records_by_ids(
    database_file: &Path,
    ids: &[String],
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if !database_file.exists() || ids.is_empty() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let sql = format!(
        "SELECT ID, NAME \
         FROM READLIST \
         WHERE ID IN ({})",
        placeholder_list(ids.len())
    );
    let mut query = sqlx::query(&sql);
    for id in ids {
        query = query.bind(id);
    }

    let rows = query.fetch_all(&pool).await?;
    let mut records = rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect::<Vec<_>>();

    records.sort_by_cached_key(|record| unicode_collation_sort_key(record.name.as_str()));

    Ok(records)
}

pub async fn load_series_search_records_limited(
    database_file: &Path,
    limit: i64,
) -> Result<Vec<PersistedSeriesSearchRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM SERIES s \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE s.DELETED_DATE IS NULL \
         ORDER BY TITLE COLLATE NOCASE ASC, s.ID ASC \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesSearchRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_book_search_records_limited(
    database_file: &Path,
    limit: i64,
) -> Result<Vec<PersistedBookSearchRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         ORDER BY TITLE COLLATE NOCASE ASC, b.ID ASC \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedBookSearchRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
        })
        .collect())
}

pub async fn load_collection_search_records_limited(
    database_file: &Path,
    limit: i64,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ID, NAME \
         FROM COLLECTION \
         ORDER BY NAME COLLATE NOCASE ASC, ID ASC \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    let mut records = rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect::<Vec<_>>();

    records.sort_by_cached_key(|record| unicode_collation_sort_key(record.name.as_str()));

    Ok(records)
}

pub async fn load_readlist_search_records_limited(
    database_file: &Path,
    limit: i64,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ID, NAME \
         FROM READLIST \
         ORDER BY NAME COLLATE NOCASE ASC, ID ASC \
         LIMIT ?",
    )
    .bind(limit)
    .fetch_all(&pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect())
}

pub async fn load_publishers(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
) -> Result<Vec<String>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT DISTINCT sm.PUBLISHER AS PUBLISHER, s.LIBRARY_ID AS LIBRARY_ID \
         FROM SERIES_METADATA sm \
         JOIN SERIES s ON s.ID = sm.SERIES_ID \
         WHERE sm.PUBLISHER IS NOT NULL \
         AND trim(sm.PUBLISHER) != '' \
         ORDER BY lower(sm.PUBLISHER), sm.PUBLISHER",
    )
    .fetch_all(&pool)
    .await?;

    let mut values = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let library_id = row.get::<String, _>("LIBRARY_ID");
        let visible = match allowed_library_ids {
            None => true,
            Some(ids) => ids.contains(&library_id),
        };
        if !visible {
            continue;
        }
        let publisher = row.get::<String, _>("PUBLISHER");
        if seen.insert(publisher.clone()) {
            values.push(publisher);
        }
    }

    values.sort_by_cached_key(|value| unicode_collation_sort_key(value));

    Ok(values)
}

fn unicode_collation_sort_key(value: &str) -> String {
    value
        .nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn tertiary_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for OPDS collection sorting should construct")
}

pub async fn load_collections(
    database_file: &Path,
    library_id: Option<&str>,
) -> Result<Vec<PersistedNamedRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            "SELECT DISTINCT c.ID, c.NAME, c.ORDERED, \
                    COALESCE(c.LAST_MODIFIED_DATE, c.CREATED_DATE, '') AS LAST_MODIFIED \
             FROM COLLECTION c \
             JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID \
             JOIN SERIES s ON s.ID = cs.SERIES_ID \
             WHERE s.LIBRARY_ID = ? \
             ORDER BY c.NAME COLLATE NOCASE ASC, c.ID ASC",
        )
        .bind(library_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            "SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
             FROM COLLECTION \
             ORDER BY NAME COLLATE NOCASE ASC, ID ASC",
        )
        .fetch_all(&pool)
        .await?
    };

    let mut records = rows
        .into_iter()
        .map(|row| PersistedNamedRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            last_modified: row
                .try_get::<String, _>("LAST_MODIFIED")
                .unwrap_or_default(),
            ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
        })
        .collect::<Vec<_>>();

    let collator = tertiary_unicode_collator();
    records.sort_by(|left, right| {
        let ordering = collator.compare(left.name.as_str(), right.name.as_str());
        if ordering.is_eq() {
            left.id.cmp(&right.id)
        } else {
            ordering
        }
    });

    Ok(records)
}

pub async fn load_collection(
    database_file: &Path,
    collection_id: &str,
) -> Result<Option<PersistedNamedRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, ORDERED, COALESCE(LAST_MODIFIED_DATE, CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PersistedNamedRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        last_modified: row
            .try_get::<String, _>("LAST_MODIFIED")
            .unwrap_or_default(),
        ordered: row.try_get::<bool, _>("ORDERED").unwrap_or(false),
    }))
}

pub async fn load_collection_books(
    database_file: &Path,
    collection_id: &str,
) -> Result<Vec<PersistedBookFeedRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT b.ID, b.LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE, b.NAME AS FILE_NAME, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(b.LAST_MODIFIED_DATE, b.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION_SERIES cs \
         JOIN BOOK b ON b.SERIES_ID = cs.SERIES_ID \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         LEFT \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE cs.COLLECTION_ID = ? \
         AND b.DELETED_DATE IS NULL \
         GROUP BY b.ID, b.LIBRARY_ID, TITLE, FILE_NAME, MEDIA_TYPE, AGE_RATING, LAST_MODIFIED \
         ORDER BY cs.NUMBER ASC, COALESCE(bm.NUMBER_SORT, CAST(b.NUMBER AS REAL), 0) ASC, \
                  b.ID ASC",
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookFeedRecord {
            id: row.get::<String, _>("ID"),
            title: row.get::<String, _>("TITLE"),
            file_name: row.get::<String, _>("FILE_NAME"),
            media_type: row.get::<String, _>("MEDIA_TYPE"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            age_rating: parsed_age_rating(&row),
            sharing_labels: parsed_sharing_labels(&row),
            last_modified: row.get::<String, _>("LAST_MODIFIED"),
        })
        .collect())
}

pub async fn load_collection_series(
    database_file: &Path,
    collection_id: &str,
    ordered: bool,
) -> Result<Vec<PersistedSeriesRecord>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1).await?;
    let query = if ordered {
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION_SERIES cs \
         JOIN SERIES s ON s.ID = cs.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE cs.COLLECTION_ID = ? \
         AND s.DELETED_DATE IS NULL \
         GROUP BY s.ID, s.LIBRARY_ID, TITLE, AGE_RATING, LAST_MODIFIED \
         ORDER BY cs.NUMBER ASC, COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, \
                  s.ID ASC"
    } else {
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, \
                COALESCE(sm.AGE_RATING, NULL) AS AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS, \
                COALESCE(s.LAST_MODIFIED_DATE, s.CREATED_DATE, '') AS LAST_MODIFIED \
         FROM COLLECTION_SERIES cs \
         JOIN SERIES s ON s.ID = cs.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE cs.COLLECTION_ID = ? \
         AND s.DELETED_DATE IS NULL \
         GROUP BY s.ID, s.LIBRARY_ID, TITLE, AGE_RATING, LAST_MODIFIED \
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC"
    };
    let rows = sqlx::query(query)
        .bind(collection_id)
        .fetch_all(&pool)
        .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedSeriesRecord {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            title: row.get::<String, _>("TITLE"),
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
