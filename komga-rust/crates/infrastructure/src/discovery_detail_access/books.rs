use sqlx::{Row, SqlitePool};

use komga_application::discovery::{
    BookReadModel, BookReadProgressReadModel, PersistedBookResourceRecord,
    PersistedBookSiblingDirectionRecord,
};
use komga_domain::discovery::MediaStatus;

use super::common;
use crate::parsing::{
    parse_metadata_authors, parse_metadata_links, parse_sqlite_group_concat_values,
};

pub(super) async fn load_book_id_by_sorted_position(
    pool: &SqlitePool,
    index: usize,
) -> Result<Option<String>, String> {
    let row = sqlx::query(
        r#"SELECT b.ID AS ID
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.DELETED_DATE IS NULL
         ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC
         LIMIT 1
         OFFSET ?"#,
    )
    .bind((index - 1) as i64)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query remapped book id: {error}"))?;

    Ok(row.map(|row| row.get::<String, _>("ID")))
}

pub(super) async fn load_persisted_book_resource(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Option<PersistedBookResourceRecord>, String> {
    let row = sqlx::query(
        r#"SELECT b.LIBRARY_ID, sm.AGE_RATING,
                COALESCE((SELECT GROUP_CONCAT(LABEL, char(30))
                          FROM (SELECT DISTINCT sms.LABEL AS LABEL
                                FROM SERIES_METADATA_SHARING sms
                                WHERE sms.SERIES_ID = s.ID)), '') AS SHARING_LABELS
         FROM BOOK b
         JOIN SERIES s ON s.ID = b.SERIES_ID
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE b.ID = ?
         GROUP BY b.LIBRARY_ID, sm.AGE_RATING"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted book resource: {error}"))?;

    Ok(row.map(|row| PersistedBookResourceRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row
            .get::<Option<i64>, _>("AGE_RATING")
            .map(common::clamp_kotlin_int_u32),
        sharing_labels: row.get::<String, _>("SHARING_LABELS"),
    }))
}

pub(super) async fn load_persisted_book_detail(
    pool: &SqlitePool,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<BookReadModel>, String> {
    let row = sqlx::query(
        r#"SELECT b.ID AS ID, b.SERIES_ID AS SERIES_ID, COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
                COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) AS SERIES_TITLE_SORT,
                b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS NAME, b.URL AS URL, b.NUMBER AS NUMBER,
                b.CREATED_DATE AS CREATED_DATE, b.LAST_MODIFIED_DATE AS LAST_MODIFIED_DATE,
                CAST(b.FILE_LAST_MODIFIED AS TEXT) AS FILE_LAST_MODIFIED,
                b.FILE_SIZE AS FILE_SIZE, b.FILE_HASH AS FILE_HASH, b.ONESHOT AS ONESHOT,
                b.DELETED_DATE AS DELETED_DATE, COALESCE(bm.TITLE, b.NAME) AS METADATA_TITLE,
                COALESCE(bm.SUMMARY, '') AS METADATA_SUMMARY,
                COALESCE(bm.NUMBER, '') AS METADATA_NUMBER,
                COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS METADATA_NUMBER_SORT,
                bm.RELEASE_DATE AS METADATA_RELEASE_DATE,
                COALESCE(bm.TITLE_LOCK, 0) AS METADATA_TITLE_LOCK,
                COALESCE(bm.SUMMARY_LOCK, 0) AS METADATA_SUMMARY_LOCK,
                COALESCE(bm.NUMBER_LOCK, 0) AS METADATA_NUMBER_LOCK,
                COALESCE(bm.NUMBER_SORT_LOCK, 0) AS METADATA_NUMBER_SORT_LOCK,
                COALESCE(bm.RELEASE_DATE_LOCK, 0) AS METADATA_RELEASE_DATE_LOCK,
                COALESCE((SELECT GROUP_CONCAT(ba.NAME || X'1E' || COALESCE(ba.ROLE, ''), X'1F')
                          FROM BOOK_METADATA_AUTHOR ba
                          WHERE ba.BOOK_ID = b.ID), '') AS METADATA_AUTHORS,
                COALESCE(bm.AUTHORS_LOCK, 0) AS METADATA_AUTHORS_LOCK,
                COALESCE((SELECT GROUP_CONCAT(bt.TAG, char(30))
                          FROM BOOK_METADATA_TAG bt
                          WHERE bt.BOOK_ID = b.ID), '') AS METADATA_TAGS,
                COALESCE(bm.TAGS_LOCK, 0) AS METADATA_TAGS_LOCK,
                COALESCE(bm.ISBN, '') AS METADATA_ISBN,
                COALESCE(bm.CREATED_DATE, b.CREATED_DATE) AS METADATA_CREATED,
                COALESCE(bm.LAST_MODIFIED_DATE, b.LAST_MODIFIED_DATE) AS METADATA_LAST_MODIFIED,
                COALESCE(bm.ISBN_LOCK, 0) AS METADATA_ISBN_LOCK,
                COALESCE((SELECT GROUP_CONCAT(bl.LABEL || X'1E' || bl.URL, X'1F')
                          FROM BOOK_METADATA_LINK bl
                          WHERE bl.BOOK_ID = b.ID), '') AS METADATA_LINKS,
                COALESCE(bm.LINKS_LOCK, 0) AS METADATA_LINKS_LOCK,
                COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS,
                COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
                COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, COALESCE(m.COMMENT, '') AS MEDIA_COMMENT,
                COALESCE(m.EPUB_DIVINA_COMPATIBLE, 0) AS EPUB_DIVINA_COMPATIBLE,
                COALESCE(m.EPUB_IS_KEPUB, 0) AS EPUB_IS_KEPUB,
                rp.PAGE AS READ_PROGRESS_PAGE, rp.COMPLETED AS READ_PROGRESS_COMPLETED,
                rp.READ_DATE AS READ_PROGRESS_READ_DATE, rp.CREATED_DATE AS READ_PROGRESS_CREATED,
                rp.LAST_MODIFIED_DATE AS READ_PROGRESS_LAST_MODIFIED,
                rp.DEVICE_ID AS READ_PROGRESS_DEVICE_ID,
                rp.DEVICE_NAME AS READ_PROGRESS_DEVICE_NAME
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         JOIN SERIES s ON s.ID = b.SERIES_ID
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
         WHERE b.ID = ?"#,
    )
    .bind(user_id.unwrap_or_default())
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted book detail: {error}"))?;

    Ok(row.map(|row| BookReadModel {
        id: row.get::<String, _>("ID"),
        series_id: row.get::<String, _>("SERIES_ID"),
        series_title: row.get::<String, _>("SERIES_TITLE"),
        series_title_sort: row.get::<String, _>("SERIES_TITLE_SORT"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        name: row.get::<String, _>("NAME"),
        url: row.get::<String, _>("URL"),
        number: row.get::<i32, _>("NUMBER"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        size_bytes: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        media_status: projected_media_status(&row),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        media_pages_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u32,
        media_comment: row.get::<String, _>("MEDIA_COMMENT"),
        media_epub_divina_compatible: row.get::<bool, _>("EPUB_DIVINA_COMPATIBLE"),
        media_epub_is_kepub: row.get::<bool, _>("EPUB_IS_KEPUB"),
        metadata_title: row.get::<String, _>("METADATA_TITLE"),
        metadata_title_lock: row.get::<bool, _>("METADATA_TITLE_LOCK"),
        metadata_summary: row.get::<String, _>("METADATA_SUMMARY"),
        metadata_summary_lock: row.get::<bool, _>("METADATA_SUMMARY_LOCK"),
        metadata_number: row.get::<String, _>("METADATA_NUMBER"),
        metadata_number_lock: row.get::<bool, _>("METADATA_NUMBER_LOCK"),
        metadata_number_sort: row.get::<f64, _>("METADATA_NUMBER_SORT"),
        metadata_number_sort_lock: row.get::<bool, _>("METADATA_NUMBER_SORT_LOCK"),
        metadata_release_date: row.get::<Option<String>, _>("METADATA_RELEASE_DATE"),
        metadata_release_date_lock: row.get::<bool, _>("METADATA_RELEASE_DATE_LOCK"),
        metadata_authors: parse_metadata_authors(&row.get::<String, _>("METADATA_AUTHORS")),
        metadata_authors_lock: row.get::<bool, _>("METADATA_AUTHORS_LOCK"),
        metadata_tags: parse_sqlite_group_concat_values(&row.get::<String, _>("METADATA_TAGS")),
        metadata_tags_lock: row.get::<bool, _>("METADATA_TAGS_LOCK"),
        metadata_isbn: row.get::<String, _>("METADATA_ISBN"),
        metadata_isbn_lock: row.get::<bool, _>("METADATA_ISBN_LOCK"),
        metadata_links: parse_metadata_links(&row.get::<String, _>("METADATA_LINKS")),
        metadata_links_lock: row.get::<bool, _>("METADATA_LINKS_LOCK"),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        read_progress: row.get::<Option<i64>, _>("READ_PROGRESS_PAGE").map(|page| {
            BookReadProgressReadModel {
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
            }
        }),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        file_hash: row.get::<String, _>("FILE_HASH"),
        oneshot: row.get::<bool, _>("ONESHOT"),
    }))
}

fn projected_media_status(row: &sqlx::sqlite::SqliteRow) -> MediaStatus {
    let raw = row.get::<String, _>("MEDIA_STATUS");
    MediaStatus::parse(&raw).expect("book media-status projection should use known values")
}

pub(super) async fn load_persisted_book_sibling_id(
    pool: &SqlitePool,
    book_id: &str,
    direction: PersistedBookSiblingDirectionRecord,
) -> Result<Option<String>, String> {
    let current = sqlx::query(
        r#"SELECT b.SERIES_ID, bm.NUMBER_SORT AS NUMBER_SORT
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.ID = ?"#,
    )
    .bind(book_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted current book for sibling lookup: {error}"))?;

    let Some(current) = current else {
        return Ok(None);
    };

    let series_id = current.get::<String, _>("SERIES_ID");
    let Some(number_sort) = current.get::<Option<f64>, _>("NUMBER_SORT") else {
        return Ok(None);
    };

    let sibling_row = match direction {
        PersistedBookSiblingDirectionRecord::Previous => {
            sqlx::query(
                r#"SELECT b.ID
                 FROM BOOK b
                 JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
                 WHERE b.SERIES_ID = ?
                 AND bm.NUMBER_SORT < ?
                 ORDER BY bm.NUMBER_SORT DESC
                 LIMIT 1"#,
            )
            .bind(&series_id)
            .bind(number_sort)
            .fetch_optional(pool)
            .await
        }
        PersistedBookSiblingDirectionRecord::Next => {
            sqlx::query(
                r#"SELECT b.ID
                 FROM BOOK b
                 JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
                 WHERE b.SERIES_ID = ?
                 AND bm.NUMBER_SORT > ?
                 ORDER BY bm.NUMBER_SORT ASC
                 LIMIT 1"#,
            )
            .bind(&series_id)
            .bind(number_sort)
            .fetch_optional(pool)
            .await
        }
    }
    .map_err(|error| format!("query persisted sibling book id: {error}"))?;

    Ok(sibling_row.map(|row| row.get::<String, _>("ID")))
}
