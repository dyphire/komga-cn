use std::path::Path;

use serde_json::{Value, json};
use sqlx::Row;

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct PageHashUnknownSource {
    pub library_root: String,
    pub book_url: String,
    pub file_name: String,
    pub media_type: String,
}

pub async fn load_page_hashes_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query("SELECT COUNT(*) AS COUNT FROM PAGE_HASH")
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT\n             ph.HASH,\n             ph.SIZE,\n             ph.ACTION,\n             ph.DELETE_COUNT,\n             ph.CREATED_DATE,\n             ph.LAST_MODIFIED_DATE,\n             COUNT(mp.BOOK_ID) AS MATCH_COUNT\n         FROM PAGE_HASH ph\n         LEFT JOIN MEDIA_PAGE mp ON mp.FILE_HASH = ph.HASH\n         GROUP BY\n             ph.HASH,\n             ph.SIZE,\n             ph.ACTION,\n             ph.DELETE_COUNT,\n             ph.CREATED_DATE,\n             ph.LAST_MODIFIED_DATE\n         ORDER BY ph.LAST_MODIFIED_DATE DESC, ph.HASH DESC\n         LIMIT ?\n         OFFSET ?",
    )
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "hash": row.get::<String, _>("HASH"),
            "size": row.get::<Option<i64>, _>("SIZE"),
            "action": row.get::<String, _>("ACTION"),
            "deleteCount": row.get::<i64, _>("DELETE_COUNT"),
            "matchCount": row.get::<i64, _>("MATCH_COUNT"),
            "created": sqlite_datetime_to_utc(&row.get::<String, _>("CREATED_DATE")),
            "lastModified": sqlite_datetime_to_utc(&row.get::<String, _>("LAST_MODIFIED_DATE")),
        })
    })
    .collect::<Vec<_>>();

    Ok(page_payload(page, size, offset, total_elements, content))
}

pub async fn load_page_hashes_unknown_page(
    database_file: &Path,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT\n         FROM (\n             SELECT mp.FILE_HASH\n             FROM MEDIA_PAGE mp\n             WHERE mp.FILE_HASH <> ''\n             AND NOT EXISTS (SELECT 1 FROM PAGE_HASH ph WHERE ph.HASH = mp.FILE_HASH)\n             GROUP BY mp.FILE_HASH\n             HAVING COUNT(mp.BOOK_ID) > 1\n         ) unknown_hashes",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT mp.FILE_HASH AS HASH, mp.FILE_SIZE AS SIZE, COUNT(mp.BOOK_ID) AS MATCH_COUNT\n         FROM MEDIA_PAGE mp\n         WHERE mp.FILE_HASH <> ''\n         AND NOT EXISTS (SELECT 1 FROM PAGE_HASH ph WHERE ph.HASH = mp.FILE_HASH)\n         GROUP BY mp.FILE_HASH, mp.FILE_SIZE\n         HAVING COUNT(mp.BOOK_ID) > 1\n         ORDER BY MATCH_COUNT DESC, HASH ASC\n         LIMIT ?\n         OFFSET ?",
    )
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "hash": row.get::<String, _>("HASH"),
            "size": row.get::<Option<i64>, _>("SIZE"),
            "matchCount": row.get::<i64, _>("MATCH_COUNT"),
        })
    })
    .collect::<Vec<_>>();

    Ok(page_payload(page, size, offset, total_elements, content))
}

pub async fn load_page_hash_matches_page(
    database_file: &Path,
    page_hash: &str,
    page: u64,
    size: u64,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query("SELECT COUNT(*) AS COUNT FROM MEDIA_PAGE WHERE FILE_HASH = ?")
        .bind(page_hash)
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let content = sqlx::query(
        "SELECT mp.BOOK_ID, b.URL, mp.NUMBER, mp.FILE_NAME, mp.FILE_SIZE, mp.MEDIA_TYPE\n         FROM MEDIA_PAGE mp\n         LEFT JOIN BOOK b ON b.ID = mp.BOOK_ID\n         WHERE mp.FILE_HASH = ?\n         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC\n         LIMIT ?\n         OFFSET ?",
    )
    .bind(page_hash)
    .bind((size.min(i64::MAX as u64)) as i64)
    .bind((offset.min(i64::MAX as u64)) as i64)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|row| {
        json!({
            "bookId": row.get::<String, _>("BOOK_ID"),
            "url": row.get::<String, _>("URL"),
            "pageNumber": row.get::<i64, _>("NUMBER") + 1,
            "fileName": row.get::<String, _>("FILE_NAME"),
            "fileSize": row.get::<Option<i64>, _>("FILE_SIZE").unwrap_or_default(),
            "mediaType": row.get::<String, _>("MEDIA_TYPE"),
        })
    })
    .collect::<Vec<_>>();

    Ok(page_payload(page, size, offset, total_elements, content))
}

pub async fn load_page_hash_thumbnail(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let thumbnail = sqlx::query("SELECT THUMBNAIL FROM PAGE_HASH_THUMBNAIL WHERE HASH = ?")
        .bind(page_hash)
        .fetch_optional(&pool)
        .await?
        .map(|row| row.get::<Vec<u8>, _>("THUMBNAIL"));
    Ok(thumbnail)
}

pub async fn load_unknown_page_hash_source(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<PageHashUnknownSource>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT\n             l.ROOT AS LIBRARY_ROOT,\n             b.URL AS BOOK_URL,\n             mp.FILE_NAME AS FILE_NAME,\n             mp.MEDIA_TYPE AS MEDIA_TYPE\n         FROM MEDIA_PAGE mp\n         INNER JOIN BOOK b ON b.ID = mp.BOOK_ID\n         INNER JOIN LIBRARY l ON l.ID = b.LIBRARY_ID\n         WHERE mp.FILE_HASH = ?\n         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC\n         LIMIT 1",
    )
    .bind(page_hash)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PageHashUnknownSource {
        library_root: row.get::<String, _>("LIBRARY_ROOT"),
        book_url: row.get::<String, _>("BOOK_URL"),
        file_name: row.get::<String, _>("FILE_NAME"),
        media_type: row
            .get::<Option<String>, _>("MEDIA_TYPE")
            .unwrap_or_else(|| "image/jpeg".to_string()),
    }))
}

fn page_payload(
    page: u64,
    size: u64,
    offset: u64,
    total_elements: u64,
    content: Vec<Value>,
) -> Value {
    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len() as u64;
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false,
            },
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    })
}

fn sqlite_datetime_to_utc(value: &str) -> String {
    if value.ends_with('Z') || value.contains('T') {
        value.to_string()
    } else {
        format!("{}Z", value.replace(' ', "T"))
    }
}
