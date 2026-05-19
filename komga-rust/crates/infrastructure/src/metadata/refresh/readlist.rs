use sqlx::{Row, SqlitePool};

use super::events;
use super::support::generated_readlist_id;

pub(super) struct ComicInfoReadListEntry {
    pub name: String,
    pub number: Option<i64>,
}

pub(super) async fn upsert_comicinfo_readlist(
    pool: &SqlitePool,
    book_id: &str,
    readlist: ComicInfoReadListEntry,
) -> Result<Option<String>, String> {
    let readlist_id = sqlx::query("SELECT ID FROM READLIST WHERE NAME = ? LIMIT 1")
        .bind(&readlist.name)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to load readlist '{}' for '{}': {error}",
                readlist.name, book_id
            )
        })?
        .map(|row| row.get::<String, _>("ID"));

    let (readlist_id, created) = match readlist_id {
        Some(readlist_id) => (readlist_id, false),
        None => {
            let generated_id = generated_readlist_id(&readlist.name);
            sqlx::query(
                "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED) VALUES (?, ?, 0, '', 1)",
            )
            .bind(&generated_id)
            .bind(&readlist.name)
            .execute(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to create ComicInfo readlist '{}' for '{}': {error}",
                    readlist.name, book_id,
                )
            })?;
            (generated_id, true)
        }
    };

    let book_already_in_readlist =
        sqlx::query("SELECT 1 FROM READLIST_BOOK WHERE READLIST_ID = ? AND BOOK_ID = ? LIMIT 1")
            .bind(&readlist_id)
            .bind(book_id)
            .fetch_optional(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to check ComicInfo readlist membership '{}' for '{}': {error}",
                    readlist.name, book_id,
                )
            })?
            .is_some();
    if book_already_in_readlist {
        return Ok(None);
    }

    let assigned_number = assign_comicinfo_readlist_number(pool, &readlist_id, readlist.number)
        .await
        .map_err(|error| {
            format!(
                "failed to assign ComicInfo readlist number '{}' for '{}': {error}",
                readlist.name, book_id,
            )
        })?;

    sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
        .bind(&readlist_id)
        .bind(book_id)
        .bind(assigned_number)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to insert ComicInfo readlist membership '{}' for '{}': {error}",
                readlist.name, book_id,
            )
        })?;

    sqlx::query(
        r#"
        UPDATE READLIST
        SET BOOK_COUNT = (SELECT COUNT(*) FROM READLIST_BOOK WHERE READLIST_ID = ?),
            LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
        WHERE ID = ?
        "#,
    )
    .bind(&readlist_id)
    .bind(&readlist_id)
    .execute(pool)
    .await
    .map_err(|error| {
        format!(
            "failed to update ComicInfo readlist counters '{}' for '{}': {error}",
            readlist.name, book_id,
        )
    })?;

    let readlist_book_ids = load_readlist_book_ids(pool, &readlist_id).await?;
    events::emit_readlist(&readlist_id, &readlist_book_ids, created);

    Ok(Some(readlist_id))
}

async fn load_readlist_book_ids(
    pool: &SqlitePool,
    readlist_id: &str,
) -> Result<Vec<String>, String> {
    sqlx::query("SELECT BOOK_ID FROM READLIST_BOOK WHERE READLIST_ID = ? ORDER BY NUMBER ASC")
        .bind(readlist_id)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to load readlist books for '{readlist_id}': {error}"))
        .map(|rows| {
            rows.into_iter()
                .map(|row| row.get::<String, _>("BOOK_ID"))
                .collect()
        })
}

async fn assign_comicinfo_readlist_number(
    pool: &SqlitePool,
    readlist_id: &str,
    requested_number: Option<i64>,
) -> Result<i64, String> {
    let max_number = sqlx::query(
        "SELECT COALESCE(MAX(NUMBER), -1) AS MAX_NUMBER FROM READLIST_BOOK WHERE READLIST_ID = ?",
    )
    .bind(readlist_id)
    .fetch_one(pool)
    .await
    .map_err(|error| format!("query ComicInfo readlist max position: {error}"))?
    .get::<i64, _>("MAX_NUMBER");

    let Some(requested_number) = requested_number else {
        return Ok(max_number + 1);
    };

    let number_taken =
        sqlx::query("SELECT 1 FROM READLIST_BOOK WHERE READLIST_ID = ? AND NUMBER = ? LIMIT 1")
            .bind(readlist_id)
            .bind(requested_number)
            .fetch_optional(pool)
            .await
            .map_err(|error| format!("query ComicInfo readlist position collision: {error}"))?
            .is_some();

    if number_taken {
        Ok(max_number + 1)
    } else {
        Ok(requested_number)
    }
}
