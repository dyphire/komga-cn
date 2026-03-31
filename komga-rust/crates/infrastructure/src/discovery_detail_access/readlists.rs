use std::path::Path as FsPath;

use crate::sqlite::connect_pool;
use sqlx::Row;

#[derive(Clone)]
pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

#[derive(Clone)]
pub struct PersistedReadlistBookRecord {
    pub book_id: String,
    pub library_id: String,
}

#[derive(Clone)]
pub struct PersistedComicrackMatchCandidateRecord {
    pub series_id: String,
    pub series_title: String,
    pub series_release_date: Option<String>,
    pub book_id: String,
    pub book_title: String,
    pub book_number: String,
}

#[derive(Clone)]
pub struct PersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}

pub async fn persisted_readlists_exist(database_file: &FsPath) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlists exists db: {error}"))?;
    let row = sqlx::query(
        "SELECT 1 AS FOUND \
         FROM READLIST \
         LIMIT 1",
    )
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted readlists existence: {error}"))?;
    Ok(row.is_some())
}

pub async fn load_persisted_readlists(
    database_file: &FsPath,
) -> Result<Vec<PersistedReadlistRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted readlists db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE \
         FROM READLIST \
         ORDER BY NAME COLLATE NOCASE ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted readlists: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistRecord {
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
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Option<PersistedReadlistRecord>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted readlist detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE \
         FROM READLIST \
         WHERE ID = ?",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted readlist detail: {error}"))?;

    Ok(row.map(|row| PersistedReadlistRecord {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        summary: row.get::<String, _>("SUMMARY"),
        ordered: row.get::<bool, _>("ORDERED"),
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
    }))
}

pub async fn load_persisted_readlist_book_rows(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<Vec<PersistedReadlistBookRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted readlist books db: {error}"))?;

    let rows = sqlx::query(
        "SELECT rb.BOOK_ID, b.LIBRARY_ID \
         FROM READLIST_BOOK rb \
         JOIN BOOK b ON b.ID = rb.BOOK_ID \
         WHERE rb.READLIST_ID = ? \
         ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted readlist books: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedReadlistBookRecord {
            book_id: row.get::<String, _>("BOOK_ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
        })
        .collect())
}

pub async fn load_comicrack_match_candidates(
    database_file: &FsPath,
) -> Result<Vec<PersistedComicrackMatchCandidateRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open comicrack match candidates db: {error}"))?;

    let rows = sqlx::query(
        "SELECT s.ID AS SERIES_ID, \
                COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE, \
                b.ID AS BOOK_ID, \
                COALESCE(bm.TITLE, b.NAME) AS BOOK_TITLE, \
                COALESCE(bm.NUMBER, CAST(b.NUMBER AS TEXT)) AS BOOK_NUMBER, \
                bma.RELEASE_DATE AS SERIES_RELEASE_DATE \
         FROM BOOK b \
         JOIN SERIES s ON s.ID = b.SERIES_ID \
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID \
         LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID",
    )
    .fetch_all(&pool)
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
    database_file: &FsPath,
    book_id: &str,
) -> Result<Vec<PersistedBookAuthorRecord>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted book authors db: {error}"))?;

    let rows = sqlx::query(
        "SELECT NAME, COALESCE(ROLE, '') AS ROLE \
         FROM BOOK_METADATA_AUTHOR \
         WHERE BOOK_ID = ?",
    )
    .bind(book_id)
    .fetch_all(&pool)
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
    database_file: &FsPath,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> Result<(), String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist create tx: {error}"))?;

    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(readlist_id)
    .bind(name)
    .bind(book_ids.len() as i64)
    .bind(summary)
    .bind(ordered)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert persisted readlist: {error}"))?;

    replace_readlist_books(&mut tx, readlist_id, book_ids)
        .await
        .map_err(|error| format!("insert persisted readlist books: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist create tx: {error}"))?;

    Ok(())
}

pub async fn persist_readlist_update(
    database_file: &FsPath,
    readlist_id: &str,
    name: &str,
    summary: &str,
    ordered: bool,
    book_ids: &[String],
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist update db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist update tx: {error}"))?;

    let updated = sqlx::query(
        "UPDATE READLIST \
         SET NAME = ?, SUMMARY = ?, ORDERED = ?, BOOK_COUNT = ?, \
             LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
         WHERE ID = ?",
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

    replace_readlist_books(&mut tx, readlist_id, book_ids)
        .await
        .map_err(|error| format!("replace persisted readlist books: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist update tx: {error}"))?;
    Ok(true)
}

pub async fn delete_persisted_readlist(
    database_file: &FsPath,
    readlist_id: &str,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist delete tx: {error}"))?;

    sqlx::query(
        "DELETE \
         FROM THUMBNAIL_READLIST \
         WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted readlist thumbnails: {error}"))?;
    sqlx::query(
        "DELETE \
         FROM READLIST_BOOK \
         WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted readlist books: {error}"))?;

    let deleted = sqlx::query(
        "DELETE \
         FROM READLIST \
         WHERE ID = ?",
    )
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("delete persisted readlist: {error}"))?
    .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist delete tx: {error}"))?;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist delete tx: {error}"))?;
    Ok(true)
}

async fn replace_readlist_books(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    readlist_id: &str,
    book_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE \
         FROM READLIST_BOOK \
         WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .execute(&mut **tx)
    .await?;

    for (index, book_id) in book_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) \
             VALUES (?, ?, ?)",
        )
        .bind(readlist_id)
        .bind(book_id)
        .bind(index as i64)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}
