use std::collections::BTreeMap;
use std::path::Path;

use reqwest::Url;
use serde_json::{Value, json};
use sqlx::{QueryBuilder, Row, Sqlite};

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct PageHashUnknownSource {
    pub library_root: String,
    pub book_url: String,
    pub file_name: String,
    pub media_type: String,
}

#[derive(Clone, Debug)]
pub struct PageHashUnknownMatchTarget {
    pub book_id: String,
    pub page_number: u64,
}

#[derive(Clone, Debug)]
pub struct PageHashDeleteTargetPage {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

#[derive(Clone, Debug)]
pub struct PageHashDeleteTarget {
    pub book_id: String,
    pub pages: Vec<PageHashDeleteTargetPage>,
}

pub async fn load_page_hashes_page(
    database_file: &Path,
    page: u64,
    size: u64,
    actions: &[String],
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let order_by = known_page_hash_order_by(sorts);

    let mut count_query = QueryBuilder::<Sqlite>::new("SELECT COUNT(*) AS COUNT FROM PAGE_HASH ph");
    push_known_page_hash_action_filter(&mut count_query, actions);
    let total_elements = count_query
        .build()
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);

    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT \
             ph.HASH, \
             ph.SIZE, \
             ph.ACTION, \
             ph.DELETE_COUNT, \
             ph.CREATED_DATE, \
             ph.LAST_MODIFIED_DATE, \
             COUNT(mp.BOOK_ID) AS MATCH_COUNT \
         FROM PAGE_HASH ph \
         LEFT JOIN MEDIA_PAGE mp ON mp.FILE_HASH = ph.HASH",
    );
    push_known_page_hash_action_filter(&mut query, actions);
    query.push(
        " GROUP BY \
             ph.HASH, \
             ph.SIZE, \
             ph.ACTION, \
             ph.DELETE_COUNT, \
             ph.CREATED_DATE, \
             ph.LAST_MODIFIED_DATE",
    );
    if !order_by.is_empty() {
        query.push(" ORDER BY ");
        query.push(order_by.join(", "));
    }
    query.push(" LIMIT ");
    query.push_bind((size.min(i64::MAX as u64)) as i64);
    query.push(" OFFSET ");
    query.push_bind((offset.min(i64::MAX as u64)) as i64);

    let content = query
        .build()
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
                "created": sqlite_datetime_to_iso_local(&row.get::<String, _>("CREATED_DATE")),
                "lastModified": sqlite_datetime_to_iso_local(&row.get::<String, _>("LAST_MODIFIED_DATE")),
            })
        })
        .collect::<Vec<_>>();

    Ok(page_payload(
        page,
        size,
        offset,
        total_elements,
        content,
        !order_by.is_empty(),
    ))
}

pub async fn load_page_hashes_unknown_page(
    database_file: &Path,
    page: u64,
    size: u64,
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query(
        "SELECT COUNT(*) AS COUNT \
         FROM ( \
             SELECT mp.FILE_HASH \
             FROM MEDIA_PAGE mp \
             WHERE mp.FILE_HASH <> '' \
             AND NOT EXISTS (SELECT 1 FROM PAGE_HASH ph WHERE ph.HASH = mp.FILE_HASH) \
             GROUP BY mp.FILE_HASH \
             HAVING COUNT(mp.BOOK_ID) > 1 \
         ) unknown_hashes",
    )
    .fetch_one(&pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);
    let order_by = unknown_page_hash_order_by(sorts);

    let mut sql = String::from(
        "SELECT mp.FILE_HASH AS HASH, mp.FILE_SIZE AS SIZE, COUNT(mp.BOOK_ID) AS MATCH_COUNT, \
         (COUNT(mp.BOOK_ID) * mp.FILE_SIZE) AS TOTAL_SIZE \
         FROM MEDIA_PAGE mp \
         LEFT JOIN BOOK b ON b.ID = mp.BOOK_ID \
         WHERE mp.FILE_HASH <> '' \
         AND NOT EXISTS (SELECT 1 FROM PAGE_HASH ph WHERE ph.HASH = mp.FILE_HASH) \
         GROUP BY mp.FILE_HASH \
         HAVING COUNT(mp.BOOK_ID) > 1",
    );
    if !order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_by.join(", "));
    }
    sql.push_str(" LIMIT ? OFFSET ?");

    let content = sqlx::query(&sql)
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

    Ok(page_payload(
        page,
        size,
        offset,
        total_elements,
        content,
        !order_by.is_empty(),
    ))
}

pub async fn load_page_hash_matches_page(
    database_file: &Path,
    page_hash: &str,
    page: u64,
    size: u64,
    sorts: &[String],
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;

    let total_elements = sqlx::query("SELECT COUNT(*) AS COUNT FROM MEDIA_PAGE WHERE FILE_HASH = ?")
        .bind(page_hash)
        .fetch_one(&pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = size.max(1);
    let offset = page.saturating_mul(size);
    let order_by = page_hash_match_order_by(sorts)?;

    let mut sql = String::from(
        "SELECT mp.BOOK_ID, b.URL, mp.NUMBER, mp.FILE_NAME, mp.FILE_SIZE, mp.MEDIA_TYPE, \
         (SELECT COUNT(*) FROM MEDIA_PAGE mp_count WHERE mp_count.FILE_HASH = ?) AS MATCH_COUNT, \
         ((SELECT COUNT(*) FROM MEDIA_PAGE mp_total WHERE mp_total.FILE_HASH = ?) * COALESCE(mp.FILE_SIZE, 0)) AS TOTAL_SIZE \
         FROM MEDIA_PAGE mp \
         LEFT JOIN BOOK b ON b.ID = mp.BOOK_ID \
         WHERE mp.FILE_HASH = ?",
    );
    if !order_by.is_empty() {
        sql.push_str(" ORDER BY ");
        sql.push_str(&order_by.join(", "));
    }
    sql.push_str(" LIMIT ? OFFSET ?");

    let content = sqlx::query(&sql)
        .bind(page_hash)
        .bind(page_hash)
        .bind(page_hash)
        .bind((size.min(i64::MAX as u64)) as i64)
        .bind((offset.min(i64::MAX as u64)) as i64)
        .fetch_all(&pool)
        .await?
        .into_iter()
        .map(|row| -> Result<Value, sqlx::Error> {
            let raw_url = row.get::<String, _>("URL");
            let Some(file_size) = row.get::<Option<i64>, _>("FILE_SIZE") else {
                return Err(sqlx::Error::Protocol(
                    "page hash match FILE_SIZE must not be null".to_string(),
                ));
            };
            Ok(json!({
                "bookId": row.get::<String, _>("BOOK_ID"),
                "url": url_to_file_path(raw_url.as_str())?,
                "pageNumber": row.get::<i64, _>("NUMBER") + 1,
                "fileName": row.get::<String, _>("FILE_NAME"),
                "fileSize": file_size,
                "mediaType": row.get::<String, _>("MEDIA_TYPE"),
            }))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(page_payload(
        page,
        size,
        offset,
        total_elements,
        content,
        !order_by.is_empty(),
    ))
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

pub async fn load_page_hash_delete_targets(
    database_file: &Path,
    page_hash: &str,
) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT mp.BOOK_ID AS BOOK_ID, mp.FILE_HASH AS FILE_HASH, mp.NUMBER AS NUMBER, mp.FILE_NAME AS FILE_NAME, mp.MEDIA_TYPE AS MEDIA_TYPE, mp.FILE_SIZE AS FILE_SIZE \
         FROM MEDIA_PAGE mp \
         WHERE mp.FILE_HASH = ? \
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC",
    )
    .bind(page_hash)
    .fetch_all(&pool)
    .await?;

    let mut by_book = BTreeMap::<String, Vec<PageHashDeleteTargetPage>>::new();
    for row in rows {
        let book_id = row.get::<String, _>("BOOK_ID");
        by_book
            .entry(book_id)
            .or_default()
            .push(PageHashDeleteTargetPage {
                file_hash: row.get::<String, _>("FILE_HASH"),
                file_size: row.get::<i64, _>("FILE_SIZE"),
                file_name: row.get::<String, _>("FILE_NAME"),
                media_type: row.get::<String, _>("MEDIA_TYPE"),
                page_number: row.get::<i64, _>("NUMBER") + 1,
            });
    }

    Ok(by_book
        .into_iter()
        .map(|(book_id, pages)| PageHashDeleteTarget { book_id, pages })
        .collect())
}

pub async fn load_unknown_page_hash_match_target(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<PageHashUnknownMatchTarget>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT mp.BOOK_ID AS BOOK_ID, mp.NUMBER AS NUMBER \
         FROM MEDIA_PAGE mp \
         WHERE mp.FILE_HASH = ? \
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC \
         LIMIT 1",
    )
    .bind(page_hash)
    .fetch_optional(&pool)
    .await?;

    Ok(row.map(|row| PageHashUnknownMatchTarget {
        book_id: row.get::<String, _>("BOOK_ID"),
        page_number: row.get::<i64, _>("NUMBER").max(0) as u64 + 1,
    }))
}

pub async fn load_unknown_page_hash_source(
    database_file: &Path,
    page_hash: &str,
) -> Result<Option<PageHashUnknownSource>, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT \
             l.ROOT AS LIBRARY_ROOT, \
             b.URL AS BOOK_URL, \
             mp.FILE_NAME AS FILE_NAME, \
             mp.MEDIA_TYPE AS MEDIA_TYPE \
         FROM MEDIA_PAGE mp \
         INNER JOIN BOOK b ON b.ID = mp.BOOK_ID \
         INNER JOIN LIBRARY l ON l.ID = b.LIBRARY_ID \
         WHERE mp.FILE_HASH = ? \
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC \
         LIMIT 1",
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
    sorted: bool,
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
                "empty": !sorted,
                "sorted": sorted,
                "unsorted": !sorted,
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
            "empty": !sorted,
            "sorted": sorted,
            "unsorted": !sorted,
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    })
}

fn push_known_page_hash_action_filter(query: &mut QueryBuilder<Sqlite>, actions: &[String]) {
    if actions.is_empty() {
        return;
    }

    query.push(" WHERE ph.ACTION IN (");
    let mut separated = query.separated(", ");
    for action in actions {
        separated.push_bind(action.clone());
    }
    separated.push_unseparated(")");
}

fn known_page_hash_order_by(sorts: &[String]) -> Vec<String> {
    sorts
        .iter()
        .filter_map(|sort| {
            let mut parts = sort.split(',');
            let property = parts.next()?.trim();
            let direction = parts.next().unwrap_or("asc").trim();
            let column = match property {
                "hash" => "ph.HASH",
                "matchCount" => "MATCH_COUNT",
                "deleteCount" => "ph.DELETE_COUNT",
                "deleteSize" => "ph.SIZE * ph.DELETE_COUNT",
                "fileSize" => "ph.SIZE",
                "createdDate" => "ph.CREATED_DATE",
                "lastModifiedDate" => "ph.LAST_MODIFIED_DATE",
                _ => return None,
            };
            let direction = if direction.eq_ignore_ascii_case("desc") {
                "DESC"
            } else {
                "ASC"
            };
            Some(format!("{column} {direction}"))
        })
        .collect()
}

fn unknown_page_hash_order_by(sorts: &[String]) -> Vec<String> {
    sorts
        .iter()
        .filter_map(|sort| {
            let mut parts = sort.split(',');
            let property = parts.next()?.trim();
            let direction = parts.next().unwrap_or("asc").trim();
            let column = match property {
                "hash" => "HASH",
                "fileSize" => "SIZE",
                "matchCount" => "MATCH_COUNT",
                "totalSize" => "TOTAL_SIZE",
                "url" => "b.URL",
                "bookId" => "mp.BOOK_ID",
                "pageNumber" => "mp.NUMBER",
                _ => return None,
            };
            let direction = if direction.eq_ignore_ascii_case("desc") {
                "DESC"
            } else {
                "ASC"
            };
            Some(format!("{column} {direction}"))
        })
        .collect()
}

fn page_hash_match_order_by(sorts: &[String]) -> Result<Vec<String>, sqlx::Error> {
    let mut order_by = Vec::new();
    for sort in sorts {
        let mut parts = sort.split(',');
        let Some(property) = parts.next().map(str::trim) else {
            continue;
        };
        if matches!(property, "matchCount" | "totalSize") {
            return Err(sqlx::Error::Protocol(format!(
                "page hash match sort key is unsupported by Kotlin baseline: {property}",
            )));
        }
        let direction = parts.next().unwrap_or("asc").trim();
        let column = match property {
            "hash" => "mp.FILE_HASH",
            "fileSize" => "mp.FILE_SIZE",
            "url" => "b.URL",
            "bookId" => "mp.BOOK_ID",
            "pageNumber" => "mp.NUMBER",
            _ => continue,
        };
        let direction = if direction.eq_ignore_ascii_case("desc") {
            "DESC"
        } else {
            "ASC"
        };
        order_by.push(format!("{column} {direction}"));
    }
    Ok(order_by)
}

fn url_to_file_path(value: &str) -> Result<String, sqlx::Error> {
    let url = Url::parse(value).map_err(|error| sqlx::Error::Protocol(error.to_string()))?;
    if url.scheme() != "file" {
        return Err(sqlx::Error::Protocol(format!(
            "page hash match URL must use file scheme: {value}",
        )));
    }

    url.to_file_path()
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|()| sqlx::Error::Protocol(format!("invalid file url: {value}")))
}

fn sqlite_datetime_to_iso_local(value: &str) -> String {
    value.replace(' ', "T").trim_end_matches('Z').to_string()
}
