use std::path::Path as FsPath;

use crate::sqlite::connect_pool;
use sqlx::Row;

#[derive(Clone)]
pub struct PersistedBookResourceRecord {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: String,
}

#[derive(Clone)]
pub struct PersistedBookDetailRecord {
    pub id: String,
    pub series_id: String,
    pub series_title: String,
    pub library_id: String,
    pub name: String,
    pub url: String,
    pub number: i32,
    pub created: String,
    pub last_modified: String,
    pub file_last_modified: String,
    pub size_bytes: u64,
    pub media_status: String,
    pub media_type: String,
    pub media_pages_count: u32,
    pub media_comment: String,
    pub metadata_title: String,
    pub metadata_summary: String,
    pub metadata_number: String,
    pub metadata_number_sort: f64,
    pub metadata_release_date: Option<String>,
    pub metadata_title_lock: bool,
    pub metadata_summary_lock: bool,
    pub metadata_number_lock: bool,
    pub metadata_number_sort_lock: bool,
    pub metadata_release_date_lock: bool,
    pub metadata_authors: String,
    pub metadata_authors_lock: bool,
    pub metadata_tags: String,
    pub metadata_tags_lock: bool,
    pub metadata_isbn: String,
    pub metadata_isbn_lock: bool,
    pub metadata_links: String,
    pub metadata_links_lock: bool,
    pub metadata_created: String,
    pub metadata_last_modified: String,
    pub media_epub_divina_compatible: bool,
    pub media_epub_is_kepub: bool,
    pub read_progress: Option<PersistedReadProgressRecord>,
    pub deleted: bool,
    pub file_hash: String,
    pub oneshot: bool,
}

#[derive(Clone)]
pub struct PersistedReadProgressRecord {
    pub page: i32,
    pub completed: bool,
    pub read_date: Option<String>,
    pub created: String,
    pub last_modified: String,
    pub device_id: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirectionRecord {
    Previous,
    Next,
}

pub async fn load_book_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book-id remap db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.ID AS ID \
         FROM BOOK b \
         LEFT \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         WHERE b.DELETED_DATE IS NULL \
         ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC \
         LIMIT 1 \
         OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped book id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("ID")))
}

pub async fn load_persisted_book_resource(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookResourceRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book resource db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.LIBRARY_ID, sm.AGE_RATING, \
                COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS \
         FROM BOOK b \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID \
         WHERE b.ID = ? \
         GROUP BY b.LIBRARY_ID, sm.AGE_RATING",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book resource: {error}"))?;

    Ok(row.map(|row| PersistedBookResourceRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(|value| value as u16),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
    }))
}

pub async fn load_persisted_book_detail(
    database_file: &FsPath,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<PersistedBookDetailRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.ID AS ID, b.SERIES_ID AS SERIES_ID, COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE, \
                b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS NAME, b.URL AS URL, b.NUMBER AS NUMBER, \
                b.CREATED_DATE AS CREATED_DATE, b.LAST_MODIFIED_DATE AS LAST_MODIFIED_DATE, \
                CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED, \
                b.FILE_SIZE AS FILE_SIZE, b.FILE_HASH AS FILE_HASH, b.ONESHOT AS ONESHOT, \
                b.DELETED_DATE AS DELETED_DATE, bm.TITLE AS METADATA_TITLE, \
                bm.SUMMARY AS METADATA_SUMMARY, bm.NUMBER AS METADATA_NUMBER, \
                bm.NUMBER_SORT AS METADATA_NUMBER_SORT, bm.RELEASE_DATE AS METADATA_RELEASE_DATE, \
                bm.TITLE_LOCK AS METADATA_TITLE_LOCK, bm.SUMMARY_LOCK AS METADATA_SUMMARY_LOCK, \
                bm.NUMBER_LOCK AS METADATA_NUMBER_LOCK, \
                bm.NUMBER_SORT_LOCK AS METADATA_NUMBER_SORT_LOCK, \
                bm.RELEASE_DATE_LOCK AS METADATA_RELEASE_DATE_LOCK, \
                COALESCE((SELECT GROUP_CONCAT(ba.NAME || X'1E' || COALESCE(ba.ROLE, ''), X'1F') \
                          FROM BOOK_METADATA_AUTHOR ba \
                          WHERE ba.BOOK_ID = b.ID), '') AS METADATA_AUTHORS, \
                bm.AUTHORS_LOCK AS METADATA_AUTHORS_LOCK, \
                COALESCE((SELECT GROUP_CONCAT(bt.TAG) \
                          FROM BOOK_METADATA_TAG bt \
                          WHERE bt.BOOK_ID = b.ID), '') AS METADATA_TAGS, \
                bm.TAGS_LOCK AS METADATA_TAGS_LOCK, \
                bm.ISBN AS METADATA_ISBN, bm.CREATED_DATE AS METADATA_CREATED, \
                bm.LAST_MODIFIED_DATE AS METADATA_LAST_MODIFIED, \
                bm.ISBN_LOCK AS METADATA_ISBN_LOCK, \
                COALESCE((SELECT GROUP_CONCAT(bl.LABEL || X'1E' || bl.URL, X'1F') \
                          FROM BOOK_METADATA_LINK bl \
                          WHERE bl.BOOK_ID = b.ID), '') AS METADATA_LINKS, \
                bm.LINKS_LOCK AS METADATA_LINKS_LOCK, \
                COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS, \
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, \
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, COALESCE(m.COMMENT, '') AS MEDIA_COMMENT, \
                COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE, \
                COALESCE(m.EPUB_IS_KEPUB, 0) AS EPUB_IS_KEPUB, \
                rp.PAGE AS READ_PROGRESS_PAGE, rp.COMPLETED AS READ_PROGRESS_COMPLETED, \
                rp.READ_DATE AS READ_PROGRESS_READ_DATE, rp.CREATED_DATE AS READ_PROGRESS_CREATED, \
                rp.LAST_MODIFIED_DATE AS READ_PROGRESS_LAST_MODIFIED, \
                NULLIF(rp.DEVICE_ID, '') AS READ_PROGRESS_DEVICE_ID, \
                NULLIF(rp.DEVICE_NAME, '') AS READ_PROGRESS_DEVICE_NAME \
         FROM BOOK b \
         JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT \
         JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT \
         JOIN MEDIA m ON m.BOOK_ID = b.ID \
         LEFT \
         JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ? \
         WHERE b.ID = ?",
    )
    .bind(user_id.unwrap_or_default())
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book detail: {error}"))?;

    Ok(row.map(|row| PersistedBookDetailRecord {
        id: row.get::<String, _>("ID"),
        series_id: row.get::<String, _>("SERIES_ID"),
        series_title: row.get::<String, _>("SERIES_TITLE"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        name: row.get::<String, _>("NAME"),
        url: row.get::<String, _>("URL"),
        number: row.get::<i32, _>("NUMBER"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        size_bytes: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        media_status: row.get::<String, _>("MEDIA_STATUS"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        media_pages_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u32,
        media_comment: row.get::<String, _>("MEDIA_COMMENT"),
        metadata_title: row.get::<String, _>("METADATA_TITLE"),
        metadata_summary: row.get::<String, _>("METADATA_SUMMARY"),
        metadata_number: row.get::<String, _>("METADATA_NUMBER"),
        metadata_number_sort: row.get::<f64, _>("METADATA_NUMBER_SORT"),
        metadata_release_date: row.get::<Option<String>, _>("METADATA_RELEASE_DATE"),
        metadata_title_lock: row.get::<bool, _>("METADATA_TITLE_LOCK"),
        metadata_summary_lock: row.get::<bool, _>("METADATA_SUMMARY_LOCK"),
        metadata_number_lock: row.get::<bool, _>("METADATA_NUMBER_LOCK"),
        metadata_number_sort_lock: row.get::<bool, _>("METADATA_NUMBER_SORT_LOCK"),
        metadata_release_date_lock: row.get::<bool, _>("METADATA_RELEASE_DATE_LOCK"),
        metadata_authors: row.get::<String, _>("METADATA_AUTHORS"),
        metadata_authors_lock: row.get::<bool, _>("METADATA_AUTHORS_LOCK"),
        metadata_tags: row.get::<String, _>("METADATA_TAGS"),
        metadata_tags_lock: row.get::<bool, _>("METADATA_TAGS_LOCK"),
        metadata_isbn: row.get::<String, _>("METADATA_ISBN"),
        metadata_isbn_lock: row.get::<bool, _>("METADATA_ISBN_LOCK"),
        metadata_links: row.get::<String, _>("METADATA_LINKS"),
        metadata_links_lock: row.get::<bool, _>("METADATA_LINKS_LOCK"),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        media_epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
        media_epub_is_kepub: row.get::<bool, _>("EPUB_IS_KEPUB"),
        read_progress: row.get::<Option<i64>, _>("READ_PROGRESS_PAGE").map(|page| {
            PersistedReadProgressRecord {
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
                device_id: row.get::<Option<String>, _>("READ_PROGRESS_DEVICE_ID"),
                device_name: row.get::<Option<String>, _>("READ_PROGRESS_DEVICE_NAME"),
            }
        }),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        file_hash: row.get::<String, _>("FILE_HASH"),
        oneshot: row.get::<bool, _>("ONESHOT"),
    }))
}

pub async fn load_persisted_book_sibling_id(
    database_file: &FsPath,
    book_id: &str,
    direction: PersistedBookSiblingDirectionRecord,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book sibling db: {error}"))?;

    let current = sqlx::query(
        "SELECT SERIES_ID, NUMBER \
         FROM BOOK \
         WHERE ID = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted current book for sibling lookup: {error}"))?;

    let Some(current) = current else {
        return Ok(None);
    };

    let series_id = current.get::<String, _>("SERIES_ID");
    let number = current.get::<i32, _>("NUMBER");

    let sibling_row = match direction {
        PersistedBookSiblingDirectionRecord::Previous => {
            sqlx::query(
                "SELECT ID \
                 FROM BOOK \
                 WHERE SERIES_ID = ? \
                 AND DELETED_DATE IS NULL \
                 AND (NUMBER < ? \
                 OR (NUMBER = ? \
                 AND ID < ?)) \
                 ORDER BY NUMBER DESC, ID DESC \
                 LIMIT 1",
            )
            .bind(&series_id)
            .bind(number)
            .bind(number)
            .bind(book_id)
            .fetch_optional(&pool)
            .await
        }
        PersistedBookSiblingDirectionRecord::Next => {
            sqlx::query(
                "SELECT ID \
                 FROM BOOK \
                 WHERE SERIES_ID = ? \
                 AND DELETED_DATE IS NULL \
                 AND (NUMBER > ? \
                 OR (NUMBER = ? \
                 AND ID > ?)) \
                 ORDER BY NUMBER ASC, ID ASC \
                 LIMIT 1",
            )
            .bind(&series_id)
            .bind(number)
            .bind(number)
            .bind(book_id)
            .fetch_optional(&pool)
            .await
        }
    }
    .map_err(|error| format!("query persisted sibling book id: {error}"))?;

    Ok(sibling_row.map(|row| row.get::<String, _>("ID")))
}
