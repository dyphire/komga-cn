use sqlx::{Row, SqlitePool};

use super::common;

pub use komga_application::discovery::{
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    PersistedBookAuthorRecord, PersistedComicrackMatchCandidateRecord,
};

pub async fn persisted_readlists_exist(pool: &SqlitePool) -> Result<bool, String> {
    common::table_has_rows(pool, "READLIST", "persisted readlists").await
}

pub async fn load_persisted_readlists(
    pool: &SqlitePool,
) -> Result<Vec<DiscoveryPersistedReadlistRecord>, String> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM READLIST
ORDER BY NAME COLLATE NOCASE ASC"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted readlists: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| DiscoveryPersistedReadlistRecord {
            id: row.get::<String, _>("ID"),
            name: row.get::<String, _>("NAME"),
            summary: row.get::<String, _>("SUMMARY"),
            ordered: row.get::<bool, _>("ORDERED"),
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
        })
        .collect())
}

pub async fn load_persisted_readlist_detail(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Option<DiscoveryPersistedReadlistRecord>, String> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM READLIST
WHERE ID = ?"#,
    )
    .bind(readlist_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("query persisted readlist detail: {error}"))?;

    Ok(row.map(|row| DiscoveryPersistedReadlistRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        summary: row.get::<String, _>("SUMMARY"),
        ordered: row.get::<bool, _>("ORDERED"),
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
    }))
}

pub async fn load_persisted_readlist_book_rows(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Vec<DiscoveryPersistedReadlistBookRecord>, String> {
    let rows = sqlx::query(
        r#"SELECT rb.BOOK_ID, b.LIBRARY_ID
FROM READLIST_BOOK rb
JOIN BOOK b ON b.ID = rb.BOOK_ID
WHERE rb.READLIST_ID = ?
ORDER BY rb.NUMBER ASC"#,
    )
    .bind(readlist_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted readlist books: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| DiscoveryPersistedReadlistBookRecord {
            book_id: row.get::<String, _>("BOOK_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
        })
        .collect())
}

pub async fn load_comicrack_match_candidates(
    pool: &SqlitePool,
) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
    let rows = sqlx::query(
        r#"SELECT s.ID AS SERIES_ID,
       COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE,
       b.ID AS BOOK_ID,
       COALESCE(bm.TITLE, b.NAME) AS BOOK_TITLE,
       COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT)) AS BOOK_NUMBER,
       bma.RELEASE_DATE AS SERIES_RELEASE_DATE
FROM BOOK b
JOIN SERIES s ON s.ID = b.SERIES_ID
LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query comicrack match candidates: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedComicrackMatchCandidateRecord {
            series_id: row.get::<String, _>("SERIES_ID"),
            series_title: row.get::<String, _>("SERIES_TITLE"),
            series_release_date: row.get::<Option<String>, _>("SERIES_RELEASE_DATE"),
            book_id: row.get::<String, _>("BOOK_ID"),
            book_title: row.get::<String, _>("BOOK_TITLE"),
            book_number: row.get::<String, _>("BOOK_NUMBER"),
        })
        .collect())
}

pub async fn load_persisted_book_authors(
    pool: &SqlitePool,
    book_id: &str,
) -> Result<Vec<PersistedBookAuthorRecord>, String> {
    let rows = sqlx::query(
        r#"SELECT NAME, COALESCE(ROLE, '') AS ROLE
FROM BOOK_METADATA_AUTHOR
WHERE BOOK_ID = ?"#,
    )
    .bind(book_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted book authors: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedBookAuthorRecord {
            name: row.get::<String, _>("NAME"),
            role: row.get::<String, _>("ROLE"),
        })
        .collect())
}

pub async fn persist_readlist_create(
    pool: &SqlitePool,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist create tx: {error}"))?;

    sqlx::query(
        r#"INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED)
VALUES (?, ?, ?, ?, ?)"#,
    )
    .bind(readlist_id)
    .bind(name)
    .bind(book_ids.len() as i64)
    .bind(summary)
    .bind(ordered)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert persisted readlist: {error}"))?;

    common::replace_ordered_children(
        &mut tx,
        "READLIST_BOOK",
        "READLIST_ID",
        "BOOK_ID",
        readlist_id,
        book_ids,
    )
    .await
    .map_err(|error| format!("insert persisted readlist books: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist create tx: {error}"))?;

    Ok(())
}

pub async fn persist_readlist_update(
    pool: &SqlitePool,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> Result<bool, String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist update tx: {error}"))?;

    let updated = sqlx::query(
        r#"UPDATE READLIST
SET NAME = ?, SUMMARY = ?, ORDERED = ?, BOOK_COUNT = ?,
    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
    )
    .bind(name)
    .bind(summary)
    .bind(ordered)
    .bind(book_ids.len() as i64)
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("update persisted readlist: {error}"))?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist update tx: {error}"))?;
        return Ok(false);
    }

    common::replace_ordered_children(
        &mut tx,
        "READLIST_BOOK",
        "READLIST_ID",
        "BOOK_ID",
        readlist_id,
        book_ids,
    )
    .await
    .map_err(|error| format!("replace persisted readlist books: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist update tx: {error}"))?;
    Ok(true)
}

pub async fn delete_persisted_readlist(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<bool, String> {
    common::delete_parent_with_children(
        pool,
        "THUMBNAIL_READLIST",
        "READLIST_BOOK",
        "READLIST",
        "READLIST_ID",
        readlist_id,
        "readlist",
    )
    .await
}
