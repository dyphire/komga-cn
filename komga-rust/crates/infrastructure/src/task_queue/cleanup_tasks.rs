use sqlx::{Row, Sqlite, SqlitePool};
use unicode_normalization::{UnicodeNormalization, char::is_combining_mark};

use super::*;
use crate::sql::task_queue::{EMPTY_TRASH_BOOK_DEPENDENCY_SQL, EMPTY_TRASH_SERIES_DEPENDENCY_SQL};

pub(super) async fn empty_trash(
    runtime: &RuntimeConfig,
    library_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    empty_trash_rows(&runtime.task_write_pool, library_id)
        .await
        .map_err(TaskExecutionError::runtime)
}

pub(super) async fn cleanup_empty_sets(runtime: &RuntimeConfig) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    if !runtime.owns_main_database {
        return Ok(());
    }

    cleanup_empty_sets_rows(&runtime.task_write_pool)
        .await
        .map_err(TaskExecutionError::runtime)
}

#[derive(Clone, Debug)]
struct PersistedCleanupEmptySetsFlags {
    delete_collections: bool,
    delete_readlists: bool,
}

pub async fn empty_trash_rows(pool: &SqlitePool, library_id: &str) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start empty-trash transaction: {error}"))?;

    let affected_series_ids = load_empty_trash_affected_series_ids(&mut tx, library_id)
        .await
        .map_err(|error| {
            format!(
                "failed to load affected series for empty-trash library '{library_id}': {error}"
            )
        })?;

    for sql in EMPTY_TRASH_BOOK_DEPENDENCY_SQL {
        sqlx::query(sql)
            .bind(library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!(
                    "failed to delete empty-trash dependent rows for library '{library_id}': {error}"
                )
            })?;
    }

    sqlx::query(
        r#"
        DELETE FROM BOOK
        WHERE LIBRARY_ID = ?
        AND DELETED_DATE IS NOT NULL
        "#,
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to delete trashed BOOK rows for library '{library_id}': {error}")
    })?;

    sqlx::query(
        r#"
        UPDATE SERIES
        SET BOOK_COUNT = (
        SELECT COUNT(*)
        FROM BOOK
        WHERE BOOK.SERIES_ID = SERIES.ID
        )
        WHERE LIBRARY_ID = ?
        "#,
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to refresh SERIES book counts for library '{library_id}': {error}")
    })?;

    for sql in EMPTY_TRASH_SERIES_DEPENDENCY_SQL {
        sqlx::query(sql)
            .bind(library_id)
            .execute(&mut *tx)
            .await
            .map_err(|error| {
                format!(
                    "failed to delete empty-trash SERIES dependents for library '{library_id}': {error}"
                )
            })?;
    }

    sqlx::query(
        r#"
        DELETE FROM SERIES
        WHERE LIBRARY_ID = ?
        AND DELETED_DATE IS NOT NULL
        "#,
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| {
        format!("failed to delete trashed SERIES rows for library '{library_id}': {error}")
    })?;

    resort_empty_trash_affected_series(&mut tx, &affected_series_ids)
        .await
        .map_err(|error| {
            format!(
                "failed to resort affected series after empty-trash for library '{library_id}': {error}"
            )
        })?;

    tx.commit().await.map_err(|error| {
        format!("failed to commit empty-trash transaction for library '{library_id}': {error}")
    })?;

    Ok(())
}

async fn load_empty_trash_affected_series_ids(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    library_id: &str,
) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT SERIES_ID
        FROM BOOK
        WHERE LIBRARY_ID = ?
          AND DELETED_DATE IS NOT NULL
        ORDER BY SERIES_ID ASC
        "#,
    )
    .bind(library_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|error| format!("query affected series ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("SERIES_ID"))
        .collect())
}

async fn resort_empty_trash_affected_series(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    series_ids: &[String],
) -> Result<(), String> {
    for series_id in series_ids {
        let exists = sqlx::query(
            r#"
            SELECT 1 AS FOUND
            FROM SERIES
            WHERE ID = ?
              AND DELETED_DATE IS NULL
            LIMIT 1
            "#,
        )
        .bind(series_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(|error| format!("query affected series existence '{series_id}': {error}"))?
        .is_some();
        if !exists {
            continue;
        }

        let book_rows = sqlx::query(
            r#"
            SELECT b.ID AS BOOK_ID,
                   b.NAME AS BOOK_NAME,
                   b.NUMBER AS BOOK_NUMBER
            FROM BOOK b
            WHERE b.SERIES_ID = ?
              AND b.DELETED_DATE IS NULL
            ORDER BY b.ID ASC
            "#,
        )
        .bind(series_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(|error| format!("query affected series books '{series_id}': {error}"))?;

        let mut books = book_rows
            .into_iter()
            .map(|row| EmptyTrashSortableBook {
                id: row.get::<String, _>("BOOK_ID"),
                name: row.get::<String, _>("BOOK_NAME"),
                number: row.get::<i64, _>("BOOK_NUMBER"),
            })
            .collect::<Vec<_>>();
        books.sort_by(|left, right| {
            compare_book_names_kotlin_like(&left.name, &right.name)
                .then_with(|| left.id.cmp(&right.id))
        });

        for (index, book) in books.iter().enumerate() {
            let new_number = index as i64 + 1;

            if book.number != new_number {
                sqlx::query(
                    r#"
                    UPDATE BOOK
                    SET NUMBER = ?,
                        LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                    WHERE ID = ?
                    "#,
                )
                .bind(new_number)
                .bind(&book.id)
                .execute(&mut **tx)
                .await
                .map_err(|error| format!("update book order '{}': {error}", book.id))?;
            }

            sqlx::query(
                r#"
                UPDATE BOOK_METADATA
                SET NUMBER = CASE WHEN NUMBER_LOCK = 0 THEN ? ELSE NUMBER END,
                    NUMBER_SORT = CASE WHEN NUMBER_SORT_LOCK = 0 THEN ? ELSE NUMBER_SORT END,
                    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
                WHERE BOOK_ID = ?
                "#,
            )
            .bind(new_number.to_string())
            .bind(new_number as f64)
            .bind(&book.id)
            .execute(&mut **tx)
            .await
            .map_err(|error| format!("update book metadata order '{}': {error}", book.id))?;
        }
    }

    Ok(())
}

struct EmptyTrashSortableBook {
    id: String,
    name: String,
    number: i64,
}

pub(crate) fn compare_book_names_kotlin_like(left: &str, right: &str) -> std::cmp::Ordering {
    let left = normalized_empty_trash_sort_key(left);
    let right = normalized_empty_trash_sort_key(right);
    natural_cmp(left.as_str(), right.as_str())
}

fn normalized_empty_trash_sort_key(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .nfd()
        .filter(|ch| !is_combining_mark(*ch))
        .flat_map(|ch| ch.to_lowercase())
        .collect()
}

fn natural_cmp(left: &str, right: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left = left.as_bytes();
    let right = right.as_bytes();
    let mut left_index = 0usize;
    let mut right_index = 0usize;

    while left_index < left.len() && right_index < right.len() {
        let left_is_digit = left[left_index].is_ascii_digit();
        let right_is_digit = right[right_index].is_ascii_digit();

        if left_is_digit && right_is_digit {
            let left_end = digit_run_end(left, left_index);
            let right_end = digit_run_end(right, right_index);
            let ordering =
                compare_digit_runs(&left[left_index..left_end], &right[right_index..right_end]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let ordering = left[left_index].cmp(&right[right_index]);
        if ordering != Ordering::Equal {
            return ordering;
        }
        left_index += 1;
        right_index += 1;
    }

    left.len().cmp(&right.len())
}

fn digit_run_end(bytes: &[u8], start: usize) -> usize {
    let mut index = start;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    index
}

fn compare_digit_runs(left: &[u8], right: &[u8]) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left_trimmed = trim_leading_zeroes(left);
    let right_trimmed = trim_leading_zeroes(right);
    let significant = left_trimmed.len().cmp(&right_trimmed.len());
    if significant != Ordering::Equal {
        return significant;
    }

    let lexical = left_trimmed.cmp(right_trimmed);
    if lexical != Ordering::Equal {
        return lexical;
    }

    left.len().cmp(&right.len())
}

fn trim_leading_zeroes(bytes: &[u8]) -> &[u8] {
    let first_non_zero = bytes.iter().position(|byte| *byte != b'0');
    match first_non_zero {
        Some(index) => &bytes[index..],
        None if bytes.is_empty() => bytes,
        None => &bytes[bytes.len() - 1..],
    }
}

pub async fn cleanup_empty_sets_rows(pool: &SqlitePool) -> Result<(), String> {
    let flags = load_cleanup_empty_sets_flags_from_pool(pool).await?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start cleanup-empty-sets transaction: {error}"))?;

    let mut deletes = Vec::<&str>::new();
    if flags.delete_collections {
        deletes.push(
            "DELETE FROM THUMBNAIL_COLLECTION WHERE COLLECTION_ID IN (SELECT ID FROM COLLECTION WHERE ID NOT IN (SELECT COLLECTION_ID FROM COLLECTION_SERIES))",
        );
        deletes.push(
            "DELETE FROM COLLECTION WHERE ID NOT IN (SELECT COLLECTION_ID FROM COLLECTION_SERIES)",
        );
    }
    if flags.delete_readlists {
        deletes.push(
            "DELETE FROM THUMBNAIL_READLIST WHERE READLIST_ID IN (SELECT ID FROM READLIST WHERE ID NOT IN (SELECT READLIST_ID FROM READLIST_BOOK))",
        );
        deletes
            .push("DELETE FROM READLIST WHERE ID NOT IN (SELECT READLIST_ID FROM READLIST_BOOK)");
    }

    for sql in deletes {
        sqlx::query(sql)
            .execute(&mut *tx)
            .await
            .map_err(|error| format!("failed to cleanup empty sets rows: {error}"))?;
    }

    tx.commit()
        .await
        .map_err(|error| format!("failed to commit cleanup-empty-sets transaction: {error}"))?;

    Ok(())
}

async fn load_cleanup_empty_sets_flags_from_pool(
    pool: &SqlitePool,
) -> Result<PersistedCleanupEmptySetsFlags, String> {
    let rows = sqlx::query(
        r#"
        SELECT KEY, VALUE
        FROM SERVER_SETTINGS
        WHERE KEY IN ('DELETE_EMPTY_COLLECTIONS', 'DELETE_EMPTY_READLISTS')
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load cleanup-empty-sets flags from server settings: {error}")
    })?;

    let mut delete_collections = false;
    let mut delete_readlists = false;

    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<Option<String>, _>("VALUE").unwrap_or_default();
        let enabled = value.trim().eq_ignore_ascii_case("true");
        match key.as_str() {
            "DELETE_EMPTY_COLLECTIONS" => delete_collections = enabled,
            "DELETE_EMPTY_READLISTS" => delete_readlists = enabled,
            _ => {}
        }
    }

    Ok(PersistedCleanupEmptySetsFlags {
        delete_collections,
        delete_readlists,
    })
}
