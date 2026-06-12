use std::collections::BTreeMap;

use komga_application::operational::{
    PageHashAction, PageHashDeleteTarget, PageHashDeleteTargetPage, PageHashKnownEntry,
    PageHashKnownQuery, PageHashKnownSortProperty, PageHashMatchEntry, PageHashMatchSortProperty,
    PageHashMatchesQuery, PageHashPage, PageHashSort, PageHashSortDirection, PageHashUnknownEntry,
    PageHashUnknownQuery, PageHashUnknownSortProperty,
};
use reqwest::Url;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

use super::super::page_hash_action::{
    parse_persisted_page_hash_action, persisted_page_hash_action,
};

#[derive(Clone, Debug)]
pub(crate) struct PageHashUnknownSource {
    pub(crate) library_root: String,
    pub(crate) book_url: String,
    pub(crate) file_name: String,
    pub(crate) media_type: String,
}

#[derive(Clone, Debug)]
pub(crate) struct PageHashUnknownMatchTarget {
    pub(crate) book_id: String,
    pub(crate) page_number: u64,
}

pub(crate) async fn load_page_hashes_page(
    pool: &SqlitePool,
    request: PageHashKnownQuery,
) -> Result<PageHashPage<PageHashKnownEntry>, sqlx::Error> {
    let order_by = known_page_hash_order_by(&request.sorts);

    let mut count_query =
        QueryBuilder::<Sqlite>::new(r#"SELECT COUNT(*) AS COUNT FROM PAGE_HASH ph"#);
    push_known_page_hash_action_filter(&mut count_query, &request.actions);
    let total_elements = count_query
        .build()
        .fetch_one(pool)
        .await?
        .get::<i64, _>("COUNT") as u64;

    let size = request.size.max(1);
    let offset = request.page.saturating_mul(size);

    let mut query = QueryBuilder::<Sqlite>::new(
        r#"SELECT
             ph.HASH,
             ph.SIZE,
             ph.ACTION,
             ph.DELETE_COUNT,
             ph.CREATED_DATE,
             ph.LAST_MODIFIED_DATE,
             COUNT(mp.BOOK_ID) AS MATCH_COUNT
         FROM PAGE_HASH ph
         LEFT JOIN MEDIA_PAGE mp ON mp.FILE_HASH = ph.HASH"#,
    );
    push_known_page_hash_action_filter(&mut query, &request.actions);
    query.push(
        r#" GROUP BY
             ph.HASH,
             ph.SIZE,
             ph.ACTION,
             ph.DELETE_COUNT,
             ph.CREATED_DATE,
             ph.LAST_MODIFIED_DATE"#,
    );
    if !order_by.is_empty() {
        query.push(r#" ORDER BY "#);
        query.push(order_by.join(", "));
    }
    query.push(r#" LIMIT "#);
    query.push_bind((size.min(i64::MAX as u64)) as i64);
    query.push(r#" OFFSET "#);
    query.push_bind((offset.min(i64::MAX as u64)) as i64);

    let content = query
        .build()
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| -> Result<PageHashKnownEntry, sqlx::Error> {
            let raw_action = row.get::<String, _>("ACTION");
            Ok(PageHashKnownEntry {
                hash: row.get::<String, _>("HASH"),
                size: row.get::<Option<i64>, _>("SIZE"),
                action: parse_persisted_page_hash_action(raw_action.as_str()).ok_or_else(|| {
                    sqlx::Error::Protocol(format!("unsupported page hash action: {raw_action}"))
                })?,
                delete_count: row.get::<i64, _>("DELETE_COUNT"),
                match_count: row.get::<i64, _>("MATCH_COUNT"),
                created: sqlite_datetime_to_iso_local(&row.get::<String, _>("CREATED_DATE")),
                last_modified: sqlite_datetime_to_iso_local(
                    &row.get::<String, _>("LAST_MODIFIED_DATE"),
                ),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PageHashPage::new(
        request.page,
        size,
        total_elements,
        content,
        !order_by.is_empty(),
    ))
}

pub(crate) async fn load_page_hashes_unknown_page(
    pool: &SqlitePool,
    request: PageHashUnknownQuery,
) -> Result<PageHashPage<PageHashUnknownEntry>, sqlx::Error> {
    let total_elements = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
         FROM (
             SELECT mp.FILE_HASH
             FROM MEDIA_PAGE mp
             WHERE mp.FILE_HASH <> ''
             AND NOT EXISTS (SELECT 1 FROM PAGE_HASH ph WHERE ph.HASH = mp.FILE_HASH)
             GROUP BY mp.FILE_HASH
             HAVING COUNT(mp.BOOK_ID) > 1
         ) unknown_hashes"#,
    )
    .fetch_one(pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = request.size.max(1);
    let offset = request.page.saturating_mul(size);
    let order_by = unknown_page_hash_order_by(&request.sorts);

    let mut sql = String::from(
        r#"SELECT mp.FILE_HASH AS HASH, mp.FILE_SIZE AS SIZE, COUNT(mp.BOOK_ID) AS MATCH_COUNT,
         (COUNT(mp.BOOK_ID) * mp.FILE_SIZE) AS TOTAL_SIZE
         FROM MEDIA_PAGE mp
         LEFT JOIN BOOK b ON b.ID = mp.BOOK_ID
         WHERE mp.FILE_HASH <> ''
         AND NOT EXISTS (SELECT 1 FROM PAGE_HASH ph WHERE ph.HASH = mp.FILE_HASH)
         GROUP BY mp.FILE_HASH
         HAVING COUNT(mp.BOOK_ID) > 1"#,
    );
    if !order_by.is_empty() {
        sql.push_str(r#" ORDER BY "#);
        sql.push_str(&order_by.join(", "));
    }
    sql.push_str(r#" LIMIT ? OFFSET ?"#);

    let content = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind((size.min(i64::MAX as u64)) as i64)
        .bind((offset.min(i64::MAX as u64)) as i64)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| PageHashUnknownEntry {
            hash: row.get::<String, _>("HASH"),
            size: row.get::<Option<i64>, _>("SIZE"),
            match_count: row.get::<i64, _>("MATCH_COUNT"),
        })
        .collect::<Vec<_>>();

    Ok(PageHashPage::new(
        request.page,
        size,
        total_elements,
        content,
        !order_by.is_empty(),
    ))
}

pub(crate) async fn load_page_hash_matches_page(
    pool: &SqlitePool,
    request: PageHashMatchesQuery,
) -> Result<PageHashPage<PageHashMatchEntry>, sqlx::Error> {
    let total_elements = sqlx::query(
        r#"SELECT COUNT(*) AS COUNT
         FROM MEDIA_PAGE
         WHERE FILE_HASH = ?"#,
    )
    .bind(request.hash.as_str())
    .fetch_one(pool)
    .await?
    .get::<i64, _>("COUNT") as u64;

    let size = request.size.max(1);
    let offset = request.page.saturating_mul(size);
    let order_by = page_hash_match_order_by(&request.sorts)?;

    let mut sql = String::from(
        r#"SELECT mp.BOOK_ID, b.URL, mp.NUMBER, mp.FILE_NAME, mp.FILE_SIZE, mp.MEDIA_TYPE,
         (SELECT COUNT(*) FROM MEDIA_PAGE mp_count WHERE mp_count.FILE_HASH = ?) AS MATCH_COUNT,
         ((SELECT COUNT(*) FROM MEDIA_PAGE mp_total WHERE mp_total.FILE_HASH = ?) * COALESCE(mp.FILE_SIZE, 0)) AS TOTAL_SIZE
         FROM MEDIA_PAGE mp
         LEFT JOIN BOOK b ON b.ID = mp.BOOK_ID
         WHERE mp.FILE_HASH = ?"#,
    );
    if !order_by.is_empty() {
        sql.push_str(r#" ORDER BY "#);
        sql.push_str(&order_by.join(", "));
    }
    sql.push_str(r#" LIMIT ? OFFSET ?"#);

    let content = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(request.hash.as_str())
        .bind(request.hash.as_str())
        .bind(request.hash.as_str())
        .bind((size.min(i64::MAX as u64)) as i64)
        .bind((offset.min(i64::MAX as u64)) as i64)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| -> Result<PageHashMatchEntry, sqlx::Error> {
            let raw_url = row.get::<String, _>("URL");
            let Some(file_size) = row.get::<Option<i64>, _>("FILE_SIZE") else {
                return Err(sqlx::Error::Protocol(
                    "page hash match FILE_SIZE must not be null".to_string(),
                ));
            };
            Ok(PageHashMatchEntry {
                book_id: row.get::<String, _>("BOOK_ID"),
                url: url_to_file_path(raw_url.as_str())?,
                page_number: row.get::<i64, _>("NUMBER") + 1,
                file_name: row.get::<String, _>("FILE_NAME"),
                file_size,
                media_type: row.get::<String, _>("MEDIA_TYPE"),
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PageHashPage::new(
        request.page,
        size,
        total_elements,
        content,
        !order_by.is_empty(),
    ))
}

pub(crate) async fn load_page_hash_thumbnail(
    pool: &SqlitePool,
    page_hash: &str,
) -> Result<Option<Vec<u8>>, sqlx::Error> {
    let thumbnail = sqlx::query(
        r#"SELECT THUMBNAIL
         FROM PAGE_HASH_THUMBNAIL
         WHERE HASH = ?"#,
    )
    .bind(page_hash)
    .fetch_optional(pool)
    .await?
    .map(|row| row.get::<Vec<u8>, _>("THUMBNAIL"));
    Ok(thumbnail)
}

pub(crate) async fn load_page_hash_delete_targets(
    pool: &SqlitePool,
    page_hash: &str,
) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT
             mp.BOOK_ID AS BOOK_ID,
             mp.FILE_HASH AS FILE_HASH,
             mp.NUMBER AS NUMBER,
             mp.FILE_NAME AS FILE_NAME,
             mp.MEDIA_TYPE AS MEDIA_TYPE,
             mp.FILE_SIZE AS FILE_SIZE
         FROM MEDIA_PAGE mp
         WHERE mp.FILE_HASH = ?
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC"#,
    )
    .bind(page_hash)
    .fetch_all(pool)
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

pub(crate) async fn load_unknown_page_hash_match_target(
    pool: &SqlitePool,
    page_hash: &str,
) -> Result<Option<PageHashUnknownMatchTarget>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT mp.BOOK_ID AS BOOK_ID, mp.NUMBER AS NUMBER
         FROM MEDIA_PAGE mp
         WHERE mp.FILE_HASH = ?
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC
         LIMIT 1"#,
    )
    .bind(page_hash)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PageHashUnknownMatchTarget {
        book_id: row.get::<String, _>("BOOK_ID"),
        page_number: row.get::<i64, _>("NUMBER") as u64 + 1,
    }))
}

pub(crate) async fn load_unknown_page_hash_source(
    pool: &SqlitePool,
    page_hash: &str,
) -> Result<Option<PageHashUnknownSource>, sqlx::Error> {
    let row = sqlx::query(
        r#"SELECT
             l.ROOT AS LIBRARY_ROOT,
             b.URL AS BOOK_URL,
             mp.FILE_NAME AS FILE_NAME,
             mp.MEDIA_TYPE AS MEDIA_TYPE
         FROM MEDIA_PAGE mp
         INNER JOIN BOOK b ON b.ID = mp.BOOK_ID
         INNER JOIN LIBRARY l ON l.ID = b.LIBRARY_ID
         WHERE mp.FILE_HASH = ?
         ORDER BY mp.BOOK_ID ASC, mp.NUMBER ASC
         LIMIT 1"#,
    )
    .bind(page_hash)
    .fetch_optional(pool)
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

fn push_known_page_hash_action_filter(
    query: &mut QueryBuilder<Sqlite>,
    actions: &[PageHashAction],
) {
    if actions.is_empty() {
        return;
    }

    query.push(r#" WHERE ph.ACTION IN ("#);
    let mut separated = query.separated(", ");
    for action in actions {
        separated.push_bind(persisted_page_hash_action(*action));
    }
    separated.push_unseparated(r#")"#);
}

fn known_page_hash_order_by(sorts: &[PageHashSort<PageHashKnownSortProperty>]) -> Vec<String> {
    sorts
        .iter()
        .map(|sort| {
            let column = match sort.property {
                PageHashKnownSortProperty::Hash => "ph.HASH",
                PageHashKnownSortProperty::MatchCount => "MATCH_COUNT",
                PageHashKnownSortProperty::DeleteCount => "ph.DELETE_COUNT",
                PageHashKnownSortProperty::DeleteSize => "ph.SIZE * ph.DELETE_COUNT",
                PageHashKnownSortProperty::FileSize => "ph.SIZE",
                PageHashKnownSortProperty::CreatedDate => "ph.CREATED_DATE",
                PageHashKnownSortProperty::LastModifiedDate => "ph.LAST_MODIFIED_DATE",
            };
            format!("{column} {}", sort_direction_sql(sort.direction))
        })
        .collect()
}

fn unknown_page_hash_order_by(sorts: &[PageHashSort<PageHashUnknownSortProperty>]) -> Vec<String> {
    sorts
        .iter()
        .map(|sort| {
            let column = match sort.property {
                PageHashUnknownSortProperty::Hash => "HASH",
                PageHashUnknownSortProperty::FileSize => "SIZE",
                PageHashUnknownSortProperty::MatchCount => "MATCH_COUNT",
                PageHashUnknownSortProperty::TotalSize => "TOTAL_SIZE",
                PageHashUnknownSortProperty::Url => "b.URL",
                PageHashUnknownSortProperty::BookId => "mp.BOOK_ID",
                PageHashUnknownSortProperty::PageNumber => "mp.NUMBER",
            };
            format!("{column} {}", sort_direction_sql(sort.direction))
        })
        .collect()
}

fn page_hash_match_order_by(
    sorts: &[PageHashSort<PageHashMatchSortProperty>],
) -> Result<Vec<String>, sqlx::Error> {
    let mut order_by = Vec::new();
    for sort in sorts {
        if matches!(
            sort.property,
            PageHashMatchSortProperty::MatchCount | PageHashMatchSortProperty::TotalSize
        ) {
            return Err(sqlx::Error::Protocol(format!(
                "page hash match sort key is unsupported by Kotlin baseline: {}",
                page_hash_match_sort_key(sort.property),
            )));
        }
        let column = match sort.property {
            PageHashMatchSortProperty::Hash => "mp.FILE_HASH",
            PageHashMatchSortProperty::FileSize => "mp.FILE_SIZE",
            PageHashMatchSortProperty::Url => "b.URL",
            PageHashMatchSortProperty::BookId => "mp.BOOK_ID",
            PageHashMatchSortProperty::PageNumber => "mp.NUMBER",
            PageHashMatchSortProperty::MatchCount | PageHashMatchSortProperty::TotalSize => {
                unreachable!("aggregate sort keys return before SQL mapping")
            }
        };
        order_by.push(format!("{column} {}", sort_direction_sql(sort.direction)));
    }
    Ok(order_by)
}

fn page_hash_match_sort_key(property: PageHashMatchSortProperty) -> &'static str {
    match property {
        PageHashMatchSortProperty::Hash => "hash",
        PageHashMatchSortProperty::FileSize => "fileSize",
        PageHashMatchSortProperty::Url => "url",
        PageHashMatchSortProperty::BookId => "bookId",
        PageHashMatchSortProperty::PageNumber => "pageNumber",
        PageHashMatchSortProperty::MatchCount => "matchCount",
        PageHashMatchSortProperty::TotalSize => "totalSize",
    }
}

fn sort_direction_sql(direction: PageHashSortDirection) -> &'static str {
    match direction {
        PageHashSortDirection::Asc => "ASC",
        PageHashSortDirection::Desc => "DESC",
    }
}

fn url_to_file_path(value: &str) -> Result<String, sqlx::Error> {
    let trimmed = value.trim();
    if !trimmed.starts_with("file:") {
        return Err(sqlx::Error::Protocol(format!(
            "page hash match URL must use file scheme: {value}",
        )));
    }

    decode_file_url_path(trimmed)
        .ok_or_else(|| sqlx::Error::Protocol(format!("invalid file url: {value}")))
}

fn decode_file_url_path(value: &str) -> Option<String> {
    if let Ok(url) = Url::parse(value) {
        if url.scheme() == "file" {
            // Contract snapshots store Kotlin-style `file:/...` URLs and expect the decoded URL
            // path, not the platform-native filesystem rendering that `to_file_path()` produces
            // on Windows.
            return percent_decode_path(url.path());
        }

        return None;
    }

    value.strip_prefix("file:").and_then(percent_decode_path)
}

fn percent_decode_path(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = *bytes.get(index + 1)?;
            let lo = *bytes.get(index + 2)?;
            decoded.push((hex_value(hi)? << 4) | hex_value(lo)?);
            index += 3;
            continue;
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn sqlite_datetime_to_iso_local(value: &str) -> String {
    value.replace(' ', "T").trim_end_matches('Z').to_string()
}
