use std::path::{Path, PathBuf};

use komga_application::media_assets::{
    BookMediaRecord, BookPageRecord, content_type_from_filename,
};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;

use crate::resolve_library_item_path;
use crate::sqlite::connect_read_pool;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedMediaFileRow {
    pub file_name: String,
    pub media_type: String,
    pub sub_type: Option<String>,
}

fn persisted_page_number_to_public(number: i64) -> u64 {
    number.max(0) as u64 + 1
}

pub(crate) fn public_page_number_to_persisted(page_number: u64) -> Option<i64> {
    page_number
        .checked_sub(1)
        .and_then(|value| i64::try_from(value).ok())
}

fn map_persisted_book_page_row(row: SqliteRow) -> BookPageRecord {
    BookPageRecord {
        number: persisted_page_number_to_public(row.get::<i64, _>("NUMBER")),
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        width: row.get::<Option<i64>, _>("width"),
        height: row.get::<Option<i64>, _>("height"),
        file_size: row.get::<i64, _>("FILE_SIZE"),
    }
}

pub async fn load_persisted_book_media(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<BookMediaRecord>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book media db: {error}"))?;

    let row = sqlx::query(
        r#"SELECT b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS FILE_NAME, b.URL AS BOOK_URL,
            l.ROOT AS LIBRARY_ROOT,
            COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE,
            COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT
         FROM BOOK b
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         WHERE b.ID = ?"#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book media: {error}"))?;

    Ok(row.map(|row| BookMediaRecord {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        file_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
        file_name: row.get::<String, _>("FILE_NAME"),
        page_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u64,
    }))
}

pub async fn load_persisted_book_media_files(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book media files db: {error}"))?;

    sqlx::query("SELECT FILE_NAME FROM MEDIA_FILE WHERE BOOK_ID = ? ORDER BY FILE_NAME ASC")
        .bind(book_id)
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted book media files: {error}"))
        .map(|rows| {
            rows.into_iter()
                .map(|row| row.get::<String, _>("FILE_NAME"))
                .collect()
        })
}

pub async fn load_persisted_media_file_records(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<PersistedMediaFileRow>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book media file records db: {error}"))?;

    sqlx::query(
        r#"SELECT FILE_NAME, COALESCE(MEDIA_TYPE, '') AS MEDIA_TYPE, SUB_TYPE
         FROM MEDIA_FILE WHERE BOOK_ID = ? ORDER BY FILE_NAME ASC"#,
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted media file records: {error}"))
    .map(|rows| {
        rows.into_iter()
            .map(|row| {
                let file_name = row.get::<String, _>("FILE_NAME");
                let media_type = row.get::<String, _>("MEDIA_TYPE");
                PersistedMediaFileRow {
                    media_type: content_type_from_filename(&file_name, &media_type),
                    file_name,
                    sub_type: row.get::<Option<String>, _>("SUB_TYPE"),
                }
            })
            .collect()
    })
}

pub async fn book_media_is_ready_status(
    database_file: &Path,
    book_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open media status db: {error}"))?;
    let row = sqlx::query("SELECT STATUS FROM MEDIA WHERE BOOK_ID = ? LIMIT 1")
        .bind(book_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query media status: {error}"))?;

    Ok(row
        .map(|row| row.get::<String, _>("STATUS"))
        .is_some_and(|status| status.eq_ignore_ascii_case("READY")))
}

pub async fn load_persisted_series_thumbnail_media(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<BookMediaRecord>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series thumbnail db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT,
            COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE
         FROM BOOK b
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY b.NUMBER ASC, b.ID ASC LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series thumbnail media: {error}"))?;

    Ok(row.map(|row| BookMediaRecord {
        library_id: String::new(),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        file_path: resolve_library_item_path(
            row.get::<String, _>("LIBRARY_ROOT").as_str(),
            row.get::<String, _>("BOOK_URL").as_str(),
        ),
        file_name: row.get::<String, _>("FILE_NAME"),
        page_count: 0,
    }))
}

pub async fn load_persisted_book_pages(
    database_file: &Path,
    book_id: &str,
) -> Result<Vec<BookPageRecord>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book pages db: {error}"))?;
    let rows = sqlx::query(
        r#"SELECT NUMBER, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT,
            CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE
         FROM MEDIA_PAGE WHERE BOOK_ID = ? ORDER BY NUMBER ASC"#,
    )
    .bind(book_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted book pages: {error}"))?;
    Ok(rows.into_iter().map(map_persisted_book_page_row).collect())
}

pub async fn load_persisted_book_page_row(
    database_file: &Path,
    book_id: &str,
    page_number: u64,
) -> Result<Option<BookPageRecord>, String> {
    if !database_file.exists() {
        return Ok(None);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open single book page db: {error}"))?;

    let Some(persisted_page_number) = public_page_number_to_persisted(page_number) else {
        return Ok(None);
    };

    let row = sqlx::query(
        r#"SELECT NUMBER, FILE_NAME, MEDIA_TYPE, WIDTH, HEIGHT,
            CASE WHEN FILE_SIZE IS NULL THEN -1 ELSE FILE_SIZE END AS FILE_SIZE
         FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = ? LIMIT 1"#,
    )
    .bind(book_id)
    .bind(persisted_page_number)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query single persisted book page: {error}"))?;
    Ok(row.map(map_persisted_book_page_row))
}

pub async fn resolve_series_id_for_persisted(
    database_file: &Path,
    requested_series_id: &str,
) -> String {
    let Some(index) = requested_series_id
        .strip_prefix("series-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_series_id.to_string();
    };
    if index == 0 {
        return requested_series_id.to_string();
    }
    if matches!(
        load_persisted_series_thumbnail_media(database_file, requested_series_id).await,
        Ok(Some(_))
    ) {
        return requested_series_id.to_string();
    }
    match load_series_id_by_sorted_position(database_file, index).await {
        Ok(Some(series_id)) => series_id,
        _ => requested_series_id.to_string(),
    }
}

pub async fn resolve_book_id_for_persisted(
    database_file: &Path,
    requested_book_id: &str,
) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };
    if index == 0 {
        return requested_book_id.to_string();
    }
    if matches!(
        load_persisted_book_media(database_file, requested_book_id).await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }
    match load_book_id_by_sorted_position(database_file, index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

async fn load_series_id_by_sorted_position(
    database_file: &Path,
    index: usize,
) -> Result<Option<String>, String> {
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series-id remap db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT s.ID AS ID
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE s.DELETED_DATE IS NULL
         ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC
         LIMIT 1 OFFSET ?"#,
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped series id: {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("ID")))
}

async fn load_book_id_by_sorted_position(
    database_file: &Path,
    index: usize,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book-id remap db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT b.ID AS ID
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.DELETED_DATE IS NULL
         ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC LIMIT 1 OFFSET ?"#,
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped book id: {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("ID")))
}

pub async fn persisted_book_exists(database_file: &Path, book_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book-exists db: {error}"))?;
    Ok(
        sqlx::query("SELECT 1 AS FOUND FROM BOOK WHERE ID = ? LIMIT 1")
            .bind(book_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("query persisted book existence: {error}"))?
            .is_some(),
    )
}

pub async fn persisted_series_exists(
    database_file: &Path,
    series_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series exists db: {error}"))?;
    Ok(
        sqlx::query("SELECT 1 AS FOUND FROM SERIES WHERE ID = ? LIMIT 1")
            .bind(series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("query persisted series existence: {error}"))?
            .is_some(),
    )
}

pub async fn load_persisted_series_oneshot(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<bool>, String> {
    if !database_file.exists() {
        return Ok(None);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series oneshot db: {error}"))?;
    let row =
        sqlx::query("SELECT COALESCE(ONESHOT, 0) AS ONESHOT FROM SERIES WHERE ID = ? LIMIT 1")
            .bind(series_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("query persisted series oneshot: {error}"))?;
    Ok(row.map(|row| row.get::<i64, _>("ONESHOT") != 0))
}

pub async fn load_series_library_id(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<String>, String> {
    if !database_file.exists() {
        return Ok(None);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series library db: {error}"))?;
    let row = sqlx::query("SELECT LIBRARY_ID FROM SERIES WHERE ID = ? LIMIT 1")
        .bind(series_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query series library id: {error}"))?;
    Ok(row.map(|row| row.get::<String, _>("LIBRARY_ID")))
}

pub async fn load_series_book_ids(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series books db: {error}"))?;
    let rows = sqlx::query(
        r#"SELECT b.ID AS ID
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY COALESCE(bm.NUMBER_SORT, 0) ASC, b.ID ASC"#,
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series book ids: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

pub async fn load_series_book_number_sorts(
    database_file: &Path,
    series_id: &str,
) -> Result<Vec<(String, f64)>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series number sort db: {error}"))?;
    let rows = sqlx::query(
        r#"SELECT b.ID AS ID, COALESCE(bm.NUMBER_SORT, 0) AS NUMBER_SORT
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY COALESCE(bm.NUMBER_SORT, 0) ASC, b.ID ASC"#,
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series number sort rows: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<String, _>("ID"), row.get::<f64, _>("NUMBER_SORT")))
        .collect())
}

pub async fn load_book_restrictions(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book restrictions db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT sm.AGE_RATING AS AGE_RATING, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS LABELS
         FROM BOOK b
         JOIN SERIES s ON s.ID = b.SERIES_ID
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID
         WHERE b.ID = ?
         GROUP BY sm.AGE_RATING"#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query book restrictions: {error}"))?;

    Ok(row.map(|row| {
        let age_rating = row
            .get::<Option<i64>, _>("AGE_RATING")
            .and_then(|value| u16::try_from(value).ok());
        let labels = row
            .get::<String, _>("LABELS")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (age_rating, labels)
    }))
}

pub async fn load_persisted_manifest_book(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(String, String, String)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open manifest book db: {error}"))?;
    let row = sqlx::query(
        r#"SELECT b.LIBRARY_ID AS LIBRARY_ID, COALESCE(bm.TITLE, b.NAME) AS TITLE,
            b.NAME AS FILE_NAME,
            COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE
         FROM BOOK b
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
         LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID
         WHERE b.ID = ?"#,
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted manifest book: {error}"))?;

    Ok(row.map(|row| {
        let file_name = row.get::<String, _>("FILE_NAME");
        let media_type = row.get::<String, _>("MEDIA_TYPE");
        (
            row.get::<String, _>("LIBRARY_ID"),
            row.get::<String, _>("TITLE"),
            content_type_from_filename(&file_name, &media_type),
        )
    }))
}

pub async fn load_persisted_epub_extension_blob(
    database_file: &Path,
    book_id: &str,
) -> Result<Option<(String, Vec<u8>)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open epub extension db: {error}"))?;
    let row = sqlx::query(
        "SELECT EXTENSION_CLASS, EXTENSION_VALUE_BLOB FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query epub extension blob: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };
    let extension_class = row
        .get::<Option<String>, _>("EXTENSION_CLASS")
        .unwrap_or_default();
    let Some(blob) = row.get::<Option<Vec<u8>>, _>("EXTENSION_VALUE_BLOB") else {
        return Ok(None);
    };
    Ok(Some((extension_class, blob)))
}

pub async fn load_readlist_archive_entries(
    database_file: &Path,
    readlist_id: &str,
) -> Result<Vec<(String, PathBuf)>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open readlist archive db: {error}"))?;
    let rows = sqlx::query(
        r#"SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT
         FROM READLIST_BOOK rb
         JOIN BOOK b ON b.ID = rb.BOOK_ID
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         WHERE rb.READLIST_ID = ?
         ORDER BY rb.NUMBER ASC"#,
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query readlist archive entries: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let file_name = row.get::<String, _>("FILE_NAME");
            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            (
                file_name,
                resolve_library_item_path(library_root.as_str(), book_url.as_str()),
            )
        })
        .collect())
}

pub async fn load_series_archive_entries(
    database_file: &Path,
    series_id: &str,
) -> Result<Option<(String, String, Vec<(String, PathBuf)>)>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open series archive db: {error}"))?;
    let series_row = sqlx::query(
        r#"SELECT s.LIBRARY_ID AS LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE
         FROM SERIES s
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
         WHERE s.ID = ?
         LIMIT 1"#,
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query series archive metadata: {error}"))?;
    let Some(series_row) = series_row else {
        return Ok(None);
    };

    let library_id = series_row.get::<String, _>("LIBRARY_ID");
    let series_title = series_row.get::<String, _>("SERIES_TITLE");
    let rows = sqlx::query(
        r#"SELECT b.NAME AS FILE_NAME, b.URL AS BOOK_URL, l.ROOT AS LIBRARY_ROOT
         FROM BOOK b
         JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL
         ORDER BY b.NUMBER ASC, b.ID ASC"#,
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query series archive entries: {error}"))?;

    let entries = rows
        .into_iter()
        .map(|row| {
            let file_name = row.get::<String, _>("FILE_NAME");
            let book_url = row.get::<String, _>("BOOK_URL");
            let library_root = row.get::<String, _>("LIBRARY_ROOT");
            (
                file_name,
                resolve_library_item_path(library_root.as_str(), book_url.as_str()),
            )
        })
        .collect::<Vec<_>>();
    Ok(Some((series_title, library_id, entries)))
}

pub async fn persisted_book_ids(database_file: &Path) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(Vec::new());
    }
    let pool = connect_read_pool(database_file)
        .await
        .map_err(|error| format!("open book list db: {error}"))?;
    let rows = sqlx::query("SELECT ID FROM BOOK ORDER BY ID ASC")
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted book ids: {error}"))?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}
