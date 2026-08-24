use anyhow::Context;
use sqlx::{Row, SqlitePool};

use crate::discovery::set_persistence;

use komga_application::discovery::{
    DiscoveryPersistedReadlistBookRecord, DiscoveryPersistedReadlistRecord,
    PersistedComicrackMatchCandidateRecord,
};

pub(in crate::discovery) async fn load_persisted_readlists(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<DiscoveryPersistedReadlistRecord>> {
    let rows = sqlx::query(
        r#"SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM READLIST
ORDER BY NAME COLLATE NOCASE ASC"#,
    )
    .fetch_all(pool)
    .await
    .context("query persisted readlists")?;

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

pub(in crate::discovery) async fn load_persisted_readlist_detail(
    pool: &SqlitePool,
    readlist_id: &str,
) -> anyhow::Result<Option<DiscoveryPersistedReadlistRecord>> {
    let row = sqlx::query(
        r#"SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE
FROM READLIST
WHERE ID = ?"#,
    )
    .bind(readlist_id)
    .fetch_optional(pool)
    .await
    .context("query persisted readlist detail")?;

    Ok(row.map(|row| DiscoveryPersistedReadlistRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        summary: row.get::<String, _>("SUMMARY"),
        ordered: row.get::<bool, _>("ORDERED"),
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
    }))
}

pub(in crate::discovery) async fn load_persisted_readlist_book_rows(
    pool: &SqlitePool,
    readlist_id: &str,
) -> anyhow::Result<Vec<DiscoveryPersistedReadlistBookRecord>> {
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
    .context("query persisted readlist books")?;

    Ok(rows
        .into_iter()
        .map(|row| DiscoveryPersistedReadlistBookRecord {
            book_id: row.get::<String, _>("BOOK_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
        })
        .collect())
}

pub(in crate::discovery) async fn load_comicrack_match_candidates(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<PersistedComicrackMatchCandidateRecord>> {
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
    .context("query comicrack match candidates")?;

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

pub(in crate::discovery) async fn persist_readlist_create(
    pool: &SqlitePool,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> anyhow::Result<()> {
    let mut tx = pool.begin().await.context("begin readlist create tx")?;

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
    .context("insert persisted readlist")?;

    set_persistence::replace_ordered_children(
        &mut tx,
        "READLIST_BOOK",
        "READLIST_ID",
        "BOOK_ID",
        readlist_id,
        book_ids,
    )
    .await
    .context("insert persisted readlist books")?;

    tx.commit().await.context("commit readlist create tx")?;

    Ok(())
}

pub(in crate::discovery) async fn persist_readlist_update(
    pool: &SqlitePool,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> anyhow::Result<bool> {
    let mut tx = pool.begin().await.context("begin readlist update tx")?;

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
    .context("update persisted readlist")?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback().await.context("rollback readlist update tx")?;
        return Ok(false);
    }

    set_persistence::replace_ordered_children(
        &mut tx,
        "READLIST_BOOK",
        "READLIST_ID",
        "BOOK_ID",
        readlist_id,
        book_ids,
    )
    .await
    .context("replace persisted readlist books")?;

    tx.commit().await.context("commit readlist update tx")?;
    Ok(true)
}

pub(in crate::discovery) async fn delete_persisted_readlist(
    pool: &SqlitePool,
    readlist_id: &str,
) -> anyhow::Result<bool> {
    set_persistence::delete_parent_with_children(
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
