use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::persisted_paths::{resolve_rooted_path, resolve_stored_path};
use crate::sql::task_queue::{DELETE_BOOK_DEPENDENCY_SQL, DELETE_SERIES_DEPENDENCY_SQL};

use super::scan_models::*;
use super::scan_sse::{
    RuntimeSseEventBuffer, RuntimeSseMutationKind, emit_scanned_library_runtime_sse_events,
    record_book_runtime_sse_event, record_series_runtime_sse_event,
};

use crate::task_queue::cleanup_tasks::compare_book_names_kotlin_like;

fn compute_file_sha256(path: &Path) -> Result<String, String> {
    let bytes = fs::read(path).map_err(|error| {
        format!(
            "failed to read book file for restore '{}': {error}",
            path.display()
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher
        .finalize()
        .iter()
        .map(|value| format!("{value:02x}"))
        .collect::<String>())
}
async fn try_restore_deleted_books(
    pool: &SqlitePool,
    library_root: &Path,
    inserted_books: &[InsertedBookCandidate],
) -> Result<(Vec<String>, Vec<BookMetadataRefreshRequest>), String> {
    let mut restored_series_ids = HashSet::<String>::new();
    let mut metadata_refreshes = Vec::<BookMetadataRefreshRequest>::new();

    for inserted in inserted_books {
        let deleted_candidates = sqlx::query(
            r#"SELECT ID, FILE_HASH
FROM BOOK
WHERE DELETED_DATE IS NOT NULL
  AND FILE_SIZE = ?
  AND COALESCE(FILE_HASH, '') <> ''
ORDER BY ID ASC"#,
        )
        .bind(inserted.file_size)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to load deleted book restore candidates: {error}"))?;
        if deleted_candidates.is_empty() {
            continue;
        }

        let inserted_hash =
            compute_file_sha256(resolve_rooted_path(library_root, &inserted.book_url).as_path())?;
        sqlx::query(
            r#"UPDATE BOOK
SET FILE_HASH = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
        )
        .bind(&inserted_hash)
        .bind(&inserted.book_id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to persist inserted book hash during restore: {error}"))?;

        let Some(matched_deleted_book_id) = deleted_candidates.into_iter().find_map(|row| {
            let file_hash = row.get::<String, _>("FILE_HASH");
            (file_hash == inserted_hash).then(|| row.get::<String, _>("ID"))
        }) else {
            continue;
        };

        sqlx::query(
            r#"UPDATE MEDIA
SET BOOK_ID = ?
WHERE BOOK_ID = ?"#,
        )
        .bind(&inserted.book_id)
        .bind(&matched_deleted_book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to restore MEDIA rows for '{}': {error}",
                inserted.book_id
            )
        })?;
        sqlx::query(
            r#"UPDATE MEDIA_FILE
SET BOOK_ID = ?
WHERE BOOK_ID = ?"#,
        )
        .bind(&inserted.book_id)
        .bind(&matched_deleted_book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to restore MEDIA_FILE rows for '{}': {error}",
                inserted.book_id
            )
        })?;
        sqlx::query(
            r#"UPDATE MEDIA_PAGE
SET BOOK_ID = ?
WHERE BOOK_ID = ?"#,
        )
        .bind(&inserted.book_id)
        .bind(&matched_deleted_book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to restore MEDIA_PAGE rows for '{}': {error}",
                inserted.book_id
            )
        })?;
        sqlx::query(
            r#"UPDATE THUMBNAIL_BOOK
SET BOOK_ID = ?
WHERE BOOK_ID = ?
  AND TYPE IN ('GENERATED', 'USER_UPLOADED')"#,
        )
        .bind(&inserted.book_id)
        .bind(&matched_deleted_book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to restore THUMBNAIL_BOOK rows for '{}': {error}",
                inserted.book_id
            )
        })?;
        sqlx::query(
            r#"UPDATE READ_PROGRESS
SET BOOK_ID = ?
WHERE BOOK_ID = ?"#,
        )
        .bind(&inserted.book_id)
        .bind(&matched_deleted_book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to restore READ_PROGRESS rows for '{}': {error}",
                inserted.book_id
            )
        })?;
        sqlx::query(
            r#"UPDATE READLIST_BOOK
SET BOOK_ID = ?
WHERE BOOK_ID = ?"#,
        )
        .bind(&inserted.book_id)
        .bind(&matched_deleted_book_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!(
                "failed to restore READLIST_BOOK rows for '{}': {error}",
                inserted.book_id
            )
        })?;

        let metadata_row = sqlx::query(
            r#"SELECT TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT,
       NUMBER_SORT_LOCK, RELEASE_DATE, RELEASE_DATE_LOCK, AUTHORS_LOCK, TAGS_LOCK, ISBN,
       ISBN_LOCK, LINKS_LOCK
FROM BOOK_METADATA
WHERE BOOK_ID = ?
LIMIT 1"#,
        )
        .bind(&matched_deleted_book_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load deleted BOOK_METADATA for restore: {error}"))?;
        let inserted_metadata_row =
            sqlx::query(r#"SELECT TITLE FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1"#)
                .bind(&inserted.book_id)
                .fetch_optional(pool)
                .await
                .map_err(|error| {
                    format!("failed to load inserted BOOK_METADATA for restore: {error}")
                })?;
        if let (Some(deleted_metadata), Some(inserted_metadata)) =
            (metadata_row, inserted_metadata_row)
        {
            let deleted_title_locked = deleted_metadata.get::<bool, _>("TITLE_LOCK");
            sqlx::query(
                r#"UPDATE BOOK_METADATA
SET TITLE = ?, TITLE_LOCK = ?, SUMMARY = ?, SUMMARY_LOCK = ?, NUMBER = ?, NUMBER_LOCK = ?,
    NUMBER_SORT = ?, NUMBER_SORT_LOCK = ?, RELEASE_DATE = ?, RELEASE_DATE_LOCK = ?,
    AUTHORS_LOCK = ?, TAGS_LOCK = ?, ISBN = ?, ISBN_LOCK = ?, LINKS_LOCK = ?,
    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE BOOK_ID = ?"#,
            )
            .bind(if deleted_title_locked {
                deleted_metadata.get::<String, _>("TITLE")
            } else {
                inserted_metadata.get::<String, _>("TITLE")
            })
            .bind(deleted_title_locked)
            .bind(deleted_metadata.get::<String, _>("SUMMARY"))
            .bind(deleted_metadata.get::<bool, _>("SUMMARY_LOCK"))
            .bind(deleted_metadata.get::<String, _>("NUMBER"))
            .bind(deleted_metadata.get::<bool, _>("NUMBER_LOCK"))
            .bind(deleted_metadata.get::<f64, _>("NUMBER_SORT"))
            .bind(deleted_metadata.get::<bool, _>("NUMBER_SORT_LOCK"))
            .bind(deleted_metadata.get::<Option<String>, _>("RELEASE_DATE"))
            .bind(deleted_metadata.get::<bool, _>("RELEASE_DATE_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("AUTHORS_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("TAGS_LOCK"))
            .bind(deleted_metadata.get::<String, _>("ISBN"))
            .bind(deleted_metadata.get::<bool, _>("ISBN_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("LINKS_LOCK"))
            .bind(&inserted.book_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to restore BOOK_METADATA row for '{}': {error}",
                    inserted.book_id
                )
            })?;
            if !deleted_title_locked {
                metadata_refreshes.push(BookMetadataRefreshRequest {
                    book_id: inserted.book_id.clone(),
                    series_id: inserted.series_id.clone(),
                    capabilities: vec!["TITLE".to_string()],
                });
            }
            sqlx::query("DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?")
                .bind(&inserted.book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to clear BOOK_METADATA_AUTHOR rows during restore: {error}")
                })?;
            sqlx::query(
                r#"INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE)
SELECT ?, NAME, ROLE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ?"#,
            )
            .bind(&inserted.book_id)
            .bind(&matched_deleted_book_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore BOOK_METADATA_AUTHOR rows: {error}"))?;
            sqlx::query("DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?")
                .bind(&inserted.book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to clear BOOK_METADATA_TAG rows during restore: {error}")
                })?;
            sqlx::query(
                r#"INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG)
SELECT ?, TAG FROM BOOK_METADATA_TAG WHERE BOOK_ID = ?"#,
            )
            .bind(&inserted.book_id)
            .bind(&matched_deleted_book_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore BOOK_METADATA_TAG rows: {error}"))?;
            sqlx::query("DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?")
                .bind(&inserted.book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to clear BOOK_METADATA_LINK rows during restore: {error}")
                })?;
            sqlx::query(
                r#"INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL)
SELECT ?, LABEL, URL FROM BOOK_METADATA_LINK WHERE BOOK_ID = ?"#,
            )
            .bind(&inserted.book_id)
            .bind(&matched_deleted_book_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore BOOK_METADATA_LINK rows: {error}"))?;
        }

        for sql in DELETE_BOOK_DEPENDENCY_SQL {
            sqlx::query(sql)
                .bind(&matched_deleted_book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to delete restored legacy book dependencies: {error}")
                })?;
        }
        sqlx::query("DELETE FROM BOOK WHERE ID = ?")
            .bind(&matched_deleted_book_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to delete restored legacy BOOK row: {error}"))?;

        let progress_user_rows = sqlx::query(
            "SELECT DISTINCT USER_ID FROM READ_PROGRESS WHERE BOOK_ID = ? ORDER BY USER_ID ASC",
        )
        .bind(&inserted.book_id)
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to load restored READ_PROGRESS users: {error}"))?;
        for row in progress_user_rows {
            let user_id = row.get::<String, _>("USER_ID");
            let aggregate = sqlx::query(
                r#"SELECT COUNT(rp.BOOK_ID) AS PROGRESS_COUNT,
       COALESCE(SUM(CASE WHEN rp.COMPLETED = 1 THEN 1 ELSE 0 END), 0) AS READ_COUNT,
       COALESCE(SUM(CASE WHEN rp.COMPLETED = 0 THEN 1 ELSE 0 END), 0) AS IN_PROGRESS_COUNT,
       MAX(rp.READ_DATE) AS MOST_RECENT_READ_DATE
FROM BOOK b
LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?
WHERE b.SERIES_ID = ? AND b.DELETED_DATE IS NULL"#,
            )
            .bind(&user_id)
            .bind(&inserted.series_id)
            .fetch_one(pool)
            .await
            .map_err(|error| {
                format!("failed to aggregate restored READ_PROGRESS_SERIES rows: {error}")
            })?;
            let progress_count = aggregate.get::<i64, _>("PROGRESS_COUNT");
            if progress_count == 0 {
                sqlx::query("DELETE FROM READ_PROGRESS_SERIES WHERE SERIES_ID = ? AND USER_ID = ?")
                    .bind(&inserted.series_id)
                    .bind(&user_id)
                    .execute(pool)
                    .await
                    .map_err(|error| {
                        format!(
                            "failed to delete empty READ_PROGRESS_SERIES row after restore: {error}"
                        )
                    })?;
            } else {
                sqlx::query(
                    r#"INSERT INTO READ_PROGRESS_SERIES (SERIES_ID, USER_ID, READ_COUNT, IN_PROGRESS_COUNT, MOST_RECENT_READ_DATE, LAST_MODIFIED_DATE)
VALUES (?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
ON CONFLICT(SERIES_ID, USER_ID) DO UPDATE
SET READ_COUNT = excluded.READ_COUNT,
    IN_PROGRESS_COUNT = excluded.IN_PROGRESS_COUNT,
    MOST_RECENT_READ_DATE = excluded.MOST_RECENT_READ_DATE,
    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP"#,
                )
                .bind(&inserted.series_id)
                .bind(&user_id)
                .bind(aggregate.get::<i64, _>("READ_COUNT"))
                .bind(aggregate.get::<i64, _>("IN_PROGRESS_COUNT"))
                .bind(aggregate.get::<Option<String>, _>("MOST_RECENT_READ_DATE"))
                .execute(pool)
                .await
                .map_err(|error| format!("failed to upsert READ_PROGRESS_SERIES row after restore: {error}"))?;
            }
        }

        restored_series_ids.insert(inserted.series_id.clone());
    }

    Ok((
        restored_series_ids.into_iter().collect(),
        metadata_refreshes,
    ))
}

async fn try_restore_deleted_series(
    pool: &SqlitePool,
    library_root: &Path,
    inserted_series: &[InsertedSeriesCandidate],
) -> Result<Vec<RestoredSeriesMatch>, String> {
    let mut restored_series_ids = Vec::new();

    for inserted in inserted_series {
        if inserted.books.is_empty() {
            continue;
        }

        let deleted_series_rows = sqlx::query(
            r#"SELECT s.ID AS ID
FROM SERIES s
WHERE s.DELETED_DATE IS NOT NULL
ORDER BY s.ID ASC"#,
        )
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to load deleted series restore candidates: {error}"))?;
        if deleted_series_rows.is_empty() {
            continue;
        }

        let mut inserted_books_with_hash = Vec::<(InsertedBookCandidate, String)>::new();
        for book in &inserted.books {
            inserted_books_with_hash.push((
                book.clone(),
                compute_file_sha256(resolve_rooted_path(library_root, &book.book_url).as_path())?,
            ));
        }

        let mut matched_deleted_series_id = None::<String>;
        for deleted_series_row in deleted_series_rows {
            let deleted_series_id = deleted_series_row.get::<String, _>("ID");
            let deleted_books = sqlx::query(
                r#"SELECT ID, FILE_SIZE, FILE_HASH
FROM BOOK
WHERE SERIES_ID = ?
ORDER BY ID ASC"#,
            )
            .bind(&deleted_series_id)
            .fetch_all(pool)
            .await
            .map_err(|error| format!("failed to load deleted series books for restore: {error}"))?;
            if deleted_books.len() != inserted_books_with_hash.len() {
                continue;
            }

            let deleted_sizes = deleted_books
                .iter()
                .map(|row| row.get::<i64, _>("FILE_SIZE"))
                .collect::<Vec<_>>();
            let inserted_sizes = inserted_books_with_hash
                .iter()
                .map(|(book, _)| book.file_size)
                .collect::<Vec<_>>();
            let mut deleted_sizes_sorted = deleted_sizes.clone();
            let mut inserted_sizes_sorted = inserted_sizes.clone();
            deleted_sizes_sorted.sort();
            inserted_sizes_sorted.sort();
            if deleted_sizes_sorted != inserted_sizes_sorted {
                continue;
            }

            let deleted_hashes = deleted_books
                .iter()
                .map(|row| row.get::<String, _>("FILE_HASH"))
                .collect::<Vec<_>>();
            let inserted_hashes = inserted_books_with_hash
                .iter()
                .map(|(_, hash)| hash.clone())
                .collect::<Vec<_>>();
            let mut deleted_hashes_sorted = deleted_hashes.clone();
            let mut inserted_hashes_sorted = inserted_hashes.clone();
            deleted_hashes_sorted.sort();
            inserted_hashes_sorted.sort();
            if deleted_hashes_sorted != inserted_hashes_sorted {
                continue;
            }

            matched_deleted_series_id = Some(deleted_series_id);
            break;
        }

        let Some(deleted_series_id) = matched_deleted_series_id else {
            continue;
        };

        let deleted_series_metadata = sqlx::query(
            r#"SELECT STATUS, STATUS_LOCK, TITLE, TITLE_LOCK, TITLE_SORT, TITLE_SORT_LOCK, SUMMARY,
       SUMMARY_LOCK, READING_DIRECTION, READING_DIRECTION_LOCK, PUBLISHER, PUBLISHER_LOCK,
       AGE_RATING, AGE_RATING_LOCK, LANGUAGE, LANGUAGE_LOCK, GENRES_LOCK, TAGS_LOCK,
       TOTAL_BOOK_COUNT, TOTAL_BOOK_COUNT_LOCK, SHARING_LABELS_LOCK, LINKS_LOCK,
       ALTERNATE_TITLES_LOCK
FROM SERIES_METADATA
WHERE SERIES_ID = ?
LIMIT 1"#,
        )
        .bind(&deleted_series_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load deleted SERIES_METADATA for restore: {error}"))?;
        let inserted_series_metadata = sqlx::query(
            r#"SELECT TITLE, TITLE_SORT FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1"#,
        )
        .bind(&inserted.series_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| format!("failed to load inserted SERIES_METADATA for restore: {error}"))?;
        if let (Some(deleted_metadata), Some(inserted_metadata)) =
            (deleted_series_metadata, inserted_series_metadata)
        {
            sqlx::query(
                r#"UPDATE SERIES
SET NAME = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
            )
            .bind(&inserted.series_title)
            .bind(&inserted.series_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to touch restored SERIES row for '{}': {error}",
                    inserted.series_id
                )
            })?;
            sqlx::query(
                r#"UPDATE SERIES_METADATA
SET STATUS = ?, STATUS_LOCK = ?, TITLE = ?, TITLE_LOCK = ?, TITLE_SORT = ?, TITLE_SORT_LOCK = ?,
    SUMMARY = ?, SUMMARY_LOCK = ?, READING_DIRECTION = ?, READING_DIRECTION_LOCK = ?,
    PUBLISHER = ?, PUBLISHER_LOCK = ?, AGE_RATING = ?, AGE_RATING_LOCK = ?, LANGUAGE = ?,
    LANGUAGE_LOCK = ?, GENRES_LOCK = ?, TAGS_LOCK = ?, TOTAL_BOOK_COUNT = ?, TOTAL_BOOK_COUNT_LOCK = ?,
    SHARING_LABELS_LOCK = ?, LINKS_LOCK = ?, ALTERNATE_TITLES_LOCK = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE SERIES_ID = ?"#,
            )
            .bind(deleted_metadata.get::<String, _>("STATUS"))
            .bind(deleted_metadata.get::<bool, _>("STATUS_LOCK"))
            .bind(if deleted_metadata.get::<bool, _>("TITLE_LOCK") {
                deleted_metadata.get::<String, _>("TITLE")
            } else {
                inserted_metadata.get::<String, _>("TITLE")
            })
            .bind(deleted_metadata.get::<bool, _>("TITLE_LOCK"))
            .bind(if deleted_metadata.get::<bool, _>("TITLE_SORT_LOCK") {
                deleted_metadata.get::<String, _>("TITLE_SORT")
            } else {
                inserted_metadata.get::<String, _>("TITLE_SORT")
            })
            .bind(deleted_metadata.get::<bool, _>("TITLE_SORT_LOCK"))
            .bind(deleted_metadata.get::<String, _>("SUMMARY"))
            .bind(deleted_metadata.get::<bool, _>("SUMMARY_LOCK"))
            .bind(deleted_metadata.get::<Option<String>, _>("READING_DIRECTION"))
            .bind(deleted_metadata.get::<bool, _>("READING_DIRECTION_LOCK"))
            .bind(deleted_metadata.get::<String, _>("PUBLISHER"))
            .bind(deleted_metadata.get::<bool, _>("PUBLISHER_LOCK"))
            .bind(deleted_metadata.get::<Option<i64>, _>("AGE_RATING"))
            .bind(deleted_metadata.get::<bool, _>("AGE_RATING_LOCK"))
            .bind(deleted_metadata.get::<String, _>("LANGUAGE"))
            .bind(deleted_metadata.get::<bool, _>("LANGUAGE_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("GENRES_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("TAGS_LOCK"))
            .bind(deleted_metadata.get::<Option<i64>, _>("TOTAL_BOOK_COUNT"))
            .bind(deleted_metadata.get::<bool, _>("TOTAL_BOOK_COUNT_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("SHARING_LABELS_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("LINKS_LOCK"))
            .bind(deleted_metadata.get::<bool, _>("ALTERNATE_TITLES_LOCK"))
            .bind(&inserted.series_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore SERIES_METADATA row for '{}': {error}", inserted.series_id))?;
            for table in [
                "SERIES_METADATA_GENRE",
                "SERIES_METADATA_TAG",
                "SERIES_METADATA_SHARING",
            ] {
                sqlx::query(&format!("DELETE FROM {table} WHERE SERIES_ID = ?"))
                    .bind(&inserted.series_id)
                    .execute(pool)
                    .await
                    .map_err(|error| {
                        format!("failed to clear restored series metadata strings: {error}")
                    })?;
            }
            sqlx::query(
                r#"INSERT INTO SERIES_METADATA_GENRE (SERIES_ID, GENRE)
SELECT ?, GENRE FROM SERIES_METADATA_GENRE WHERE SERIES_ID = ?"#,
            )
            .bind(&inserted.series_id)
            .bind(&deleted_series_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore SERIES_METADATA_GENRE rows: {error}"))?;
            sqlx::query(
                r#"INSERT INTO SERIES_METADATA_TAG (SERIES_ID, TAG)
SELECT ?, TAG FROM SERIES_METADATA_TAG WHERE SERIES_ID = ?"#,
            )
            .bind(&inserted.series_id)
            .bind(&deleted_series_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore SERIES_METADATA_TAG rows: {error}"))?;
            sqlx::query(
                r#"INSERT INTO SERIES_METADATA_SHARING (SERIES_ID, LABEL)
SELECT ?, LABEL FROM SERIES_METADATA_SHARING WHERE SERIES_ID = ?"#,
            )
            .bind(&inserted.series_id)
            .bind(&deleted_series_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore SERIES_METADATA_SHARING rows: {error}"))?;
            sqlx::query("DELETE FROM SERIES_METADATA_LINK WHERE SERIES_ID = ?")
                .bind(&inserted.series_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to clear SERIES_METADATA_LINK rows during restore: {error}")
                })?;
            sqlx::query(
                r#"INSERT INTO SERIES_METADATA_LINK (SERIES_ID, LABEL, URL)
SELECT ?, LABEL, URL FROM SERIES_METADATA_LINK WHERE SERIES_ID = ?"#,
            )
            .bind(&inserted.series_id)
            .bind(&deleted_series_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to restore SERIES_METADATA_LINK rows: {error}"))?;
            sqlx::query("DELETE FROM SERIES_METADATA_ALTERNATE_TITLE WHERE SERIES_ID = ?")
                .bind(&inserted.series_id)
                .execute(pool)
                .await
                .map_err(|error| format!("failed to clear SERIES_METADATA_ALTERNATE_TITLE rows during restore: {error}"))?;
            sqlx::query(
                r#"INSERT INTO SERIES_METADATA_ALTERNATE_TITLE (SERIES_ID, LABEL, TITLE)
SELECT ?, LABEL, TITLE FROM SERIES_METADATA_ALTERNATE_TITLE WHERE SERIES_ID = ?"#,
            )
            .bind(&inserted.series_id)
            .bind(&deleted_series_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!("failed to restore SERIES_METADATA_ALTERNATE_TITLE rows: {error}")
            })?;
        }

        sqlx::query(
            r#"UPDATE THUMBNAIL_SERIES
SET SERIES_ID = ?
WHERE SERIES_ID = ?
  AND TYPE = 'USER_UPLOADED'"#,
        )
        .bind(&inserted.series_id)
        .bind(&deleted_series_id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to restore THUMBNAIL_SERIES rows: {error}"))?;
        sqlx::query(
            r#"UPDATE COLLECTION_SERIES
SET SERIES_ID = ?
WHERE SERIES_ID = ?"#,
        )
        .bind(&inserted.series_id)
        .bind(&deleted_series_id)
        .execute(pool)
        .await
        .map_err(|error| format!("failed to restore COLLECTION_SERIES rows: {error}"))?;

        restored_series_ids.push(RestoredSeriesMatch {
            inserted_series_id: inserted.series_id.clone(),
            deleted_series_id: deleted_series_id.clone(),
        });
    }

    Ok(restored_series_ids)
}

pub(crate) async fn load_library_scan_config(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<Option<LibraryScanConfig>, String> {
    let row = sqlx::query(
        r#"SELECT ROOT, SCAN_CBX, SCAN_PDF, SCAN_EPUB, SCAN_FORCE_MODIFIED_TIME, ONESHOTS_DIRECTORY
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| format!("failed to load library root: {error}"))?;

    let Some(row) = row else {
        return Ok(None);
    };

    let exclusions = sqlx::query(
        r#"SELECT EXCLUSION
FROM LIBRARY_EXCLUSIONS
WHERE LIBRARY_ID = ?"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load library exclusions for '{library_id}': {error}"))?
    .into_iter()
    .map(|row| row.get::<String, _>("EXCLUSION"))
    .collect::<Vec<_>>();

    Ok(Some(LibraryScanConfig {
        root: row.get::<String, _>("ROOT"),
        scan_cbx: row.get::<bool, _>("SCAN_CBX"),
        scan_pdf: row.get::<bool, _>("SCAN_PDF"),
        scan_epub: row.get::<bool, _>("SCAN_EPUB"),
        scan_force_modified_time: row.get::<bool, _>("SCAN_FORCE_MODIFIED_TIME"),
        oneshots_directory: row.get::<Option<String>, _>("ONESHOTS_DIRECTORY"),
        scan_directory_exclusions: exclusions,
    }))
}

pub(crate) async fn scan_library(
    pool: &SqlitePool,
    library_id: &str,
    deep_scan: bool,
) -> Result<ScannedLibrary, String> {
    let Some(scan_config) = load_library_scan_config(pool, library_id).await? else {
        return Ok(unavailable_scanned_library());
    };

    let existing_books_by_url = load_existing_scanned_books_by_url(pool, library_id).await?;
    let existing_series_by_url = load_existing_scanned_series_by_url(pool, library_id).await?;

    build_scanned_library(
        scan_config,
        existing_books_by_url,
        existing_series_by_url,
        deep_scan,
    )
}

pub(crate) fn build_scanned_library(
    scan_config: LibraryScanConfig,
    existing_books_by_url: HashMap<String, ExistingScannedBookRow>,
    existing_series_by_url: HashMap<String, ExistingScannedSeriesRow>,
    deep_scan: bool,
) -> Result<ScannedLibrary, String> {
    let oneshots_directory: Option<String> = scan_config
        .oneshots_directory
        .as_ref()
        .map(|value| value.to_ascii_lowercase());

    let root = resolve_stored_path(&scan_config.root);
    if !root.exists() {
        return Ok(unavailable_scanned_library());
    }
    let existing_books_by_url = existing_books_by_url
        .into_iter()
        .map(|(url, row)| (scanner_url_key(root.as_path(), &url), row))
        .collect::<HashMap<_, _>>();
    let existing_series_by_url = existing_series_by_url
        .into_iter()
        .map(|(url, row)| (scanner_url_key(root.as_path(), &url), row))
        .collect::<HashMap<_, _>>();

    let mut discovered = Vec::new();
    collect_series_directories(root.as_path(), &scan_config, &mut discovered)?;

    let mut sidecars = Vec::new();
    let mut series_rows = Vec::new();
    let mut book_ids = Vec::new();
    let mut changed_existing_book_ids = HashSet::new();
    let mut changed_book_candidates_by_series_id = HashMap::<String, Vec<String>>::new();
    let mut series_ids_requiring_book_sync = HashSet::new();
    let mut discovered_series_ids = HashSet::new();
    let mut discovered_book_ids = HashSet::new();

    for series_dir in discovered {
        let series_url = series_dir.to_string_lossy().to_string();
        let regular_series_id = route_safe_scanner_id("series", series_dir.as_path());
        let series_is_oneshot = oneshots_directory
            .as_ref()
            .is_some_and(|value| series_url.to_ascii_lowercase().contains(value));
        let series_dir_last_modified_unix_seconds = fs::metadata(&series_dir)
            .ok()
            .map(|value| metadata_updated_unix_seconds(&value))
            .unwrap_or(0);

        let Ok(entries) = fs::read_dir(&series_dir) else {
            continue;
        };

        let mut books = Vec::new();
        let mut changed_book_candidates = Vec::new();
        let mut sidecar_candidates = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if !metadata.is_file() {
                continue;
            }

            if is_hidden_path(path.as_path()) {
                continue;
            }

            if is_supported_book_file(path.as_path(), &scan_config) {
                let book_url = path.to_string_lossy().to_string();
                let book_url_key = scanner_url_key(root.as_path(), &book_url);
                let book_id = existing_books_by_url
                    .get(&book_url_key)
                    .map(|existing| existing.book_id.clone())
                    .unwrap_or_else(|| route_safe_scanner_id("book", path.as_path()));
                let file_last_modified_unix_seconds = metadata_updated_unix_seconds(&metadata);
                let book_name = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string();
                let file_name = path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string();

                if let Some(existing) = existing_books_by_url.get(&book_url_key)
                    && existing.file_last_modified_unix_seconds != file_last_modified_unix_seconds
                {
                    let candidate_series_id = if series_is_oneshot {
                        resolve_oneshot_series_id(&existing_books_by_url, root.as_path(), &book_url)
                    } else {
                        regular_series_id.clone()
                    };
                    changed_book_candidates.push(existing.book_id.clone());
                    changed_book_candidates_by_series_id
                        .entry(candidate_series_id)
                        .or_default()
                        .push(existing.book_id.clone());
                }

                books.push(ScannedBookRow {
                    book_id: book_id.clone(),
                    book_name,
                    book_url,
                    file_name,
                    file_size: metadata.len() as i64,
                    file_last_modified_unix_seconds,
                    oneshot: false,
                });
                book_ids.push(book_id);
                continue;
            }

            sidecar_candidates.push((path, metadata));
        }

        if books.is_empty() {
            continue;
        }

        let books_last_modified_unix_seconds = books
            .iter()
            .map(|book| book.file_last_modified_unix_seconds)
            .max()
            .unwrap_or(0);
        let series_last_modified_unix_seconds = if scan_config.scan_force_modified_time {
            series_dir_last_modified_unix_seconds.max(books_last_modified_unix_seconds)
        } else {
            series_dir_last_modified_unix_seconds
        };
        for book in &books {
            discovered_book_ids.insert(book.book_id.clone());
        }

        if series_is_oneshot {
            sidecars.extend(build_sidecars(
                &series_url,
                &books,
                &sidecar_candidates,
                false,
            ));
            for book in &books {
                let book_url_key = scanner_url_key(root.as_path(), &book.book_url);
                let series_id = resolve_oneshot_series_id(
                    &existing_books_by_url,
                    root.as_path(),
                    &book.book_url,
                );
                let existing_series = existing_series_by_url.get(&book_url_key);
                let series_changed = existing_series.is_some_and(|existing| {
                    existing.file_last_modified_unix_seconds != book.file_last_modified_unix_seconds
                });
                let should_sync_books = deep_scan || existing_series.is_none() || series_changed;
                if should_sync_books {
                    series_ids_requiring_book_sync.insert(series_id.clone());
                    if let Some(book_ids) = changed_book_candidates_by_series_id.get(&series_id) {
                        changed_existing_book_ids.extend(book_ids.iter().cloned());
                    }
                }
                discovered_series_ids.insert(series_id.clone());
                series_rows.push(ScannedSeriesRow {
                    series_id,
                    series_name: book.book_name.clone(),
                    series_url: book.book_url.clone(),
                    series_last_modified_unix_seconds: book.file_last_modified_unix_seconds,
                    oneshot: true,
                    books: vec![ScannedBookRow {
                        oneshot: true,
                        ..book.clone()
                    }],
                });
            }
            continue;
        }

        let series_id = regular_series_id;
        let existing_series =
            existing_series_by_url.get(&scanner_url_key(root.as_path(), &series_url));
        let series_changed = existing_series.is_some_and(|existing| {
            existing.file_last_modified_unix_seconds != series_last_modified_unix_seconds
        });
        let should_sync_books = deep_scan || existing_series.is_none() || series_changed;
        if should_sync_books {
            series_ids_requiring_book_sync.insert(series_id.clone());
            changed_existing_book_ids.extend(changed_book_candidates);
        }
        discovered_series_ids.insert(series_id.clone());
        let series_name = series_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();

        sidecars.extend(build_sidecars(
            &series_url,
            &books,
            &sidecar_candidates,
            true,
        ));

        series_rows.push(ScannedSeriesRow {
            series_id,
            series_name,
            series_url,
            series_last_modified_unix_seconds,
            oneshot: false,
            books,
        });
    }

    let series_ids_with_deleted_books = existing_books_by_url
        .values()
        .filter(|existing| !discovered_book_ids.contains(&existing.book_id))
        .map(|existing| existing.series_id.clone())
        .collect::<HashSet<_>>();
    for series_id in series_ids_with_deleted_books {
        series_ids_requiring_book_sync.insert(series_id.clone());
        if let Some(book_ids) = changed_book_candidates_by_series_id.get(&series_id) {
            changed_existing_book_ids.extend(book_ids.iter().cloned());
        }
    }

    Ok(ScannedLibrary {
        root_available: true,
        series_rows,
        sidecars,
        book_ids,
        changed_existing_book_ids,
        series_ids_requiring_book_sync,
        discovered_series_ids,
        discovered_book_ids,
    })
}

pub(crate) async fn library_empty_trash_after_scan(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<bool, String> {
    let value = sqlx::query(
        r#"SELECT EMPTY_TRASH_AFTER_SCAN
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        format!("failed to load empty-trash-after-scan flag for '{library_id}': {error}")
    })?
    .map(|row| row.get::<bool, _>("EMPTY_TRASH_AFTER_SCAN"))
    .unwrap_or(false);

    Ok(value)
}

pub(crate) async fn persist_scanned_library(
    pool: &SqlitePool,
    library_id: &str,
    scanned: &ScannedLibrary,
) -> Result<PersistScannedLibraryOutcome, String> {
    let library_id = library_id.to_string();
    let outcome: PersistScannedLibraryOutcome = 'outcome: {
        let mut book_metadata_refreshes = Vec::<BookMetadataRefreshRequest>::new();
        let mut runtime_events = RuntimeSseEventBuffer::default();
        let mut changed_series_ids = HashSet::<String>::new();
        let mut inserted_books = Vec::<InsertedBookCandidate>::new();
        let mut inserted_series = Vec::<InsertedSeriesCandidate>::new();
        let library_was_unavailable = sqlx::query(
            r#"SELECT UNAVAILABLE_DATE
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
        )
        .bind(&library_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            format!("failed to load library availability state for '{library_id}': {error}")
        })?
        .and_then(|row| row.get::<Option<String>, _>("UNAVAILABLE_DATE"))
        .is_some();

        if !scanned.root_available {
            sqlx::query(
                r#"UPDATE LIBRARY
SET UNAVAILABLE_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
            )
            .bind(&library_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!("failed to mark library unavailable for '{library_id}': {error}")
            })?;
            break 'outcome PersistScannedLibraryOutcome {
                renumbered_book_ids: Vec::new(),
                library_changed: !library_was_unavailable,
                changed_series_ids: Vec::new(),
                book_metadata_refreshes: Vec::new(),
                runtime_events: runtime_events.events,
            };
        }

        if library_was_unavailable {
            sqlx::query(
                r#"UPDATE LIBRARY
SET UNAVAILABLE_DATE = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
            )
            .bind(&library_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!("failed to clear library unavailable marker for '{library_id}': {error}")
            })?;
        }

        let discovered_series_ids = scanned.discovered_series_ids.clone();
        let discovered_book_ids = scanned.discovered_book_ids.clone();
        let mut active_book_ids = HashSet::<String>::new();

        if scanned.root_available {
            let existing_series = sqlx::query(
                r#"SELECT ID
FROM SERIES
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
            )
            .bind(&library_id)
            .fetch_all(pool)
            .await
            .map_err(|error| {
                format!("failed to query existing SERIES rows for '{library_id}': {error}")
            })?;
            let existing_books = sqlx::query(
                r#"SELECT ID, SERIES_ID
FROM BOOK
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
            )
            .bind(&library_id)
            .fetch_all(pool)
            .await
            .map_err(|error| {
                format!("failed to query existing BOOK rows for '{library_id}': {error}")
            })?
            .into_iter()
            .map(|row| {
                (
                    row.get::<String, _>("ID"),
                    row.get::<String, _>("SERIES_ID"),
                )
            })
            .collect::<Vec<_>>();
            active_book_ids = existing_books
                .iter()
                .map(|(book_id, _)| book_id.clone())
                .collect::<HashSet<_>>();
            let missing_series_ids = existing_series
                .into_iter()
                .map(|row| row.get::<String, _>("ID"))
                .filter(|series_id| !discovered_series_ids.contains(series_id))
                .collect::<Vec<_>>();
            let missing_series_id_set = missing_series_ids.iter().cloned().collect::<HashSet<_>>();

            for (book_id, series_id) in &existing_books {
                if discovered_book_ids.contains(book_id)
                    || !missing_series_id_set.contains(series_id)
                {
                    continue;
                }
                sqlx::query(
                    r#"UPDATE BOOK
SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
                )
                .bind(book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to soft-delete missing BOOK '{book_id}': {error}")
                })?;
                record_book_runtime_sse_event(
                    &mut runtime_events,
                    book_id,
                    series_id,
                    &library_id,
                    RuntimeSseMutationKind::Changed,
                );
                changed_series_ids.insert(series_id.clone());
            }

            for series_id in &missing_series_ids {
                sqlx::query(
                    r#"UPDATE SERIES
SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
                )
                .bind(series_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to soft-delete missing SERIES '{series_id}': {error}")
                })?;
                record_series_runtime_sse_event(
                    &mut runtime_events,
                    series_id,
                    &library_id,
                    RuntimeSseMutationKind::Changed,
                );
            }

            for (book_id, series_id) in &existing_books {
                if discovered_book_ids.contains(book_id)
                    || missing_series_id_set.contains(series_id)
                {
                    continue;
                }
                sqlx::query(
                    r#"UPDATE BOOK
SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
                )
                .bind(book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to soft-delete missing BOOK '{book_id}': {error}")
                })?;
                record_book_runtime_sse_event(
                    &mut runtime_events,
                    book_id,
                    series_id,
                    &library_id,
                    RuntimeSseMutationKind::Changed,
                );
                changed_series_ids.insert(series_id.clone());
            }
        }

        for series in &scanned.series_rows {
            let mut inserted_in_series = Vec::<InsertedBookCandidate>::new();
            let series_updated = sqlx::query(
                r#"UPDATE SERIES
SET FILE_LAST_MODIFIED = datetime(?, 'unixepoch'), NAME = ?, URL = ?, LIBRARY_ID = ?, oneshot = ?,
    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, DELETED_DATE = NULL
WHERE ID = ?
  AND (unixepoch(FILE_LAST_MODIFIED) != ?
       OR NAME != ?
       OR URL != ?
       OR LIBRARY_ID != ?
       OR COALESCE(oneshot, 0) != ?
       OR DELETED_DATE IS NOT NULL)"#,
            )
            .bind(series.series_last_modified_unix_seconds)
            .bind(&series.series_name)
            .bind(&series.series_url)
            .bind(&library_id)
            .bind(series.oneshot)
            .bind(&series.series_id)
            .bind(series.series_last_modified_unix_seconds)
            .bind(&series.series_name)
            .bind(&series.series_url)
            .bind(&library_id)
            .bind(series.oneshot)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to update SERIES rows: {error}"))?
            .rows_affected();

            if series_updated != 0 {
                changed_series_ids.insert(series.series_id.clone());
            }

            let mut series_inserted = false;
            if series_updated == 0 {
                let inserted = sqlx::query(
                        r#"INSERT OR IGNORE INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)"#,
                    )
                    .bind(&series.series_id)
                    .bind(series.series_last_modified_unix_seconds)
                    .bind(&series.series_name)
                    .bind(&series.series_url)
                    .bind(&library_id)
                    .bind(series.oneshot)
                    .execute(pool)
                    .await
                    .map_err(|error| format!("failed to insert SERIES rows: {error}"))?
                    .rows_affected();
                if inserted != 0 {
                    series_inserted = true;
                    record_series_runtime_sse_event(
                        &mut runtime_events,
                        &series.series_id,
                        &library_id,
                        RuntimeSseMutationKind::Added,
                    );
                    inserted_series.push(InsertedSeriesCandidate {
                        series_id: series.series_id.clone(),
                        series_title: series.series_name.clone(),
                        books: Vec::new(),
                    });
                }
            }

            ensure_series_metadata_seed(pool, series)
                .await
                .map_err(|error| {
                    format!(
                        "failed to ensure SERIES metadata rows for '{}': {error}",
                        series.series_id
                    )
                })?;

            let sync_books = series_inserted
                || scanned
                    .series_ids_requiring_book_sync
                    .contains(&series.series_id);
            for book in &series.books {
                let mut book_changed = false;
                if sync_books || !active_book_ids.contains(&book.book_id) {
                    let book_updated = sqlx::query(
                        r#"UPDATE BOOK
SET FILE_LAST_MODIFIED = datetime(?, 'unixepoch'), URL = ?, SERIES_ID = ?, FILE_SIZE = ?,
    LIBRARY_ID = ?, oneshot = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, DELETED_DATE = NULL
WHERE ID = ?
  AND (unixepoch(FILE_LAST_MODIFIED) != ?
       OR URL != ?
       OR SERIES_ID != ?
       OR FILE_SIZE != ?
       OR LIBRARY_ID != ?
       OR COALESCE(oneshot, 0) != ?
       OR DELETED_DATE IS NOT NULL)"#,
                    )
                    .bind(book.file_last_modified_unix_seconds)
                    .bind(&book.book_url)
                    .bind(&series.series_id)
                    .bind(book.file_size)
                    .bind(&library_id)
                    .bind(book.oneshot)
                    .bind(&book.book_id)
                    .bind(book.file_last_modified_unix_seconds)
                    .bind(&book.book_url)
                    .bind(&series.series_id)
                    .bind(book.file_size)
                    .bind(&library_id)
                    .bind(book.oneshot)
                    .execute(pool)
                    .await
                    .map_err(|error| format!("failed to update BOOK rows: {error}"))?
                    .rows_affected();

                    if book_updated != 0 {
                        book_changed = true;
                    }

                    if book_updated == 0 {
                        let inserted = sqlx::query(
                                r#"INSERT OR IGNORE INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE,
                             LIBRARY_ID, oneshot)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?)"#,
                            )
                            .bind(&book.book_id)
                            .bind(book.file_last_modified_unix_seconds)
                            .bind(&book.book_name)
                            .bind(&book.book_url)
                            .bind(&series.series_id)
                            .bind(book.file_size)
                            .bind(&library_id)
                            .bind(book.oneshot)
                            .execute(pool)
                            .await
                            .map_err(|error| format!("failed to insert BOOK rows: {error}"))?
                            .rows_affected();
                        if inserted != 0 {
                            book_changed = true;
                            record_book_runtime_sse_event(
                                &mut runtime_events,
                                &book.book_id,
                                &series.series_id,
                                &library_id,
                                RuntimeSseMutationKind::Added,
                            );
                            inserted_in_series.push(InsertedBookCandidate {
                                book_id: book.book_id.clone(),
                                book_url: book.book_url.clone(),
                                file_size: book.file_size,
                                series_id: series.series_id.clone(),
                            });
                            inserted_books.push(InsertedBookCandidate {
                                book_id: book.book_id.clone(),
                                book_url: book.book_url.clone(),
                                file_size: book.file_size,
                                series_id: series.series_id.clone(),
                            });
                            changed_series_ids.insert(series.series_id.clone());
                        }
                    }
                }

                ensure_book_metadata_seed(pool, book)
                    .await
                    .map_err(|error| {
                        format!(
                            "failed to ensure BOOK metadata rows for '{}': {error}",
                            book.book_id
                        )
                    })?;

                if book_changed {
                    let media_updated = sqlx::query(
                        r#"UPDATE MEDIA_FILE
SET FILE_SIZE = ?
WHERE FILE_NAME = ?
  AND BOOK_ID = ?"#,
                    )
                    .bind(book.file_size)
                    .bind(&book.file_name)
                    .bind(&book.book_id)
                    .execute(pool)
                    .await
                    .map_err(|error| format!("failed to update MEDIA_FILE rows: {error}"))?
                    .rows_affected();

                    if media_updated == 0 {
                        sqlx::query(
                            r#"INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE)
VALUES (?, ?, ?)"#,
                        )
                        .bind(&book.file_name)
                        .bind(&book.book_id)
                        .bind(book.file_size)
                        .execute(pool)
                        .await
                        .map_err(|error| format!("failed to insert MEDIA_FILE rows: {error}"))?;
                    }
                }
            }

            if !inserted_in_series.is_empty()
                && let Some(series_candidate) = inserted_series
                    .iter_mut()
                    .find(|candidate| candidate.series_id == series.series_id)
            {
                series_candidate.books.extend(inserted_in_series.clone());
            }
        }

        for book_id in &scanned.changed_existing_book_ids {
            sqlx::query(
                r#"UPDATE MEDIA
SET STATUS = 'OUTDATED'
WHERE BOOK_ID = ?"#,
            )
            .bind(book_id)
            .execute(pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to mark MEDIA rows outdated after deep scan for '{book_id}': {error}"
                )
            })?;
        }

        for sidecar in &scanned.sidecars {
            let sidecar_updated = sqlx::query(
                r#"UPDATE SIDECAR
SET PARENT_URL = ?, LAST_MODIFIED_TIME = datetime(?, 'unixepoch')
WHERE URL = ?
  AND LIBRARY_ID = ?"#,
            )
            .bind(&sidecar.parent_url)
            .bind(sidecar.last_modified_unix_seconds)
            .bind(&sidecar.url)
            .bind(&library_id)
            .execute(pool)
            .await
            .map_err(|error| format!("failed to update SIDECAR rows: {error}"))?
            .rows_affected();

            if sidecar_updated == 0 {
                sqlx::query(
                        r#"INSERT OR IGNORE INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID)
VALUES (?, ?, datetime(?, 'unixepoch'), ?)"#,
                    )
                    .bind(&sidecar.url)
                    .bind(&sidecar.parent_url)
                    .bind(sidecar.last_modified_unix_seconds)
                    .bind(&library_id)
                    .execute(pool)
                    .await
                    .map_err(|error| format!("failed to insert SIDECAR rows: {error}"))?;
            }
        }

        let scanned_sidecar_urls = scanned
            .sidecars
            .iter()
            .map(|sidecar| sidecar.url.clone())
            .collect::<HashSet<_>>();
        let existing_sidecar_urls = sqlx::query(r#"SELECT URL FROM SIDECAR WHERE LIBRARY_ID = ?"#)
            .bind(&library_id)
            .fetch_all(pool)
            .await
            .map_err(|error| format!("failed to load SIDECAR rows for cleanup: {error}"))?;
        for row in existing_sidecar_urls {
            let url = row.get::<String, _>("URL");
            if scanned_sidecar_urls.contains(&url) {
                continue;
            }
            sqlx::query(r#"DELETE FROM SIDECAR WHERE LIBRARY_ID = ? AND URL = ?"#)
                .bind(&library_id)
                .bind(&url)
                .execute(pool)
                .await
                .map_err(|error| format!("failed to delete stale SIDECAR row: {error}"))?;
        }

        sqlx::query(
            r#"UPDATE SERIES
SET BOOK_COUNT = (SELECT COUNT(*)
                  FROM BOOK
                  WHERE BOOK.SERIES_ID = SERIES.ID
                    AND BOOK.DELETED_DATE IS NULL)
WHERE LIBRARY_ID = ?"#,
        )
        .bind(&library_id)
        .execute(pool)
        .await
        .map_err(|error| {
            format!("failed to refresh series book counts after scan for '{library_id}': {error}")
        })?;

        let renumbered_book_ids =
                resort_scanned_series_books(pool, &discovered_series_ids).await.map_err(
                    |error| {
                        format!(
                            "failed to apply Kotlin-like series numbering after scan for '{library_id}': {error}"
                        )
                    },
                )?;
        let library_root = resolve_stored_path(
            sqlx::query("SELECT ROOT FROM LIBRARY WHERE ID = ? LIMIT 1")
                .bind(&library_id)
                .fetch_one(pool)
                .await
                .map_err(|error| {
                    format!("failed to resolve library root for restore in '{library_id}': {error}")
                })?
                .get::<String, _>("ROOT")
                .as_str(),
        );
        let restored_series_matches =
            try_restore_deleted_series(pool, library_root.as_path(), &inserted_series).await?;
        for restored in &restored_series_matches {
            changed_series_ids.insert(restored.inserted_series_id.clone());
        }
        let (restored_series_ids, restored_book_metadata_refreshes) =
            try_restore_deleted_books(pool, library_root.as_path(), &inserted_books).await?;
        changed_series_ids.extend(restored_series_ids);
        book_metadata_refreshes.extend(restored_book_metadata_refreshes);
        for restored in &restored_series_matches {
            changed_series_ids.insert(restored.inserted_series_id.clone());
            let deleted_book_ids =
                sqlx::query("SELECT ID FROM BOOK WHERE SERIES_ID = ? ORDER BY ID ASC")
                    .bind(&restored.deleted_series_id)
                    .fetch_all(pool)
                    .await
                    .map_err(|error| {
                        format!("failed to load restored legacy series books for cleanup: {error}")
                    })?;
            for deleted_book_row in deleted_book_ids {
                let deleted_book_id = deleted_book_row.get::<String, _>("ID");
                for sql in DELETE_BOOK_DEPENDENCY_SQL {
                    sqlx::query(sql)
                        .bind(&deleted_book_id)
                        .execute(pool)
                        .await
                        .map_err(|error| {
                            format!(
                                "failed to delete restored legacy series book dependencies: {error}"
                            )
                        })?;
                }
            }
            sqlx::query("DELETE FROM BOOK WHERE SERIES_ID = ?")
                .bind(&restored.deleted_series_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    format!("failed to delete restored legacy series BOOK rows: {error}")
                })?;
            for sql in DELETE_SERIES_DEPENDENCY_SQL {
                sqlx::query(sql)
                    .bind(&restored.deleted_series_id)
                    .execute(pool)
                    .await
                    .map_err(|error| {
                        format!("failed to delete restored legacy series dependencies: {error}")
                    })?;
            }
            sqlx::query("DELETE FROM SERIES WHERE ID = ?")
                .bind(&restored.deleted_series_id)
                .execute(pool)
                .await
                .map_err(|error| format!("failed to delete restored legacy SERIES row: {error}"))?;
        }

        break 'outcome PersistScannedLibraryOutcome {
            renumbered_book_ids,
            library_changed: library_was_unavailable,
            changed_series_ids: changed_series_ids.into_iter().collect(),
            book_metadata_refreshes,
            runtime_events: runtime_events.events,
        };
    };
    emit_scanned_library_runtime_sse_events(&library_id, &outcome);
    Ok(outcome)
}

async fn resort_scanned_series_books(
    pool: &SqlitePool,
    discovered_series_ids: &HashSet<String>,
) -> Result<Vec<String>, sqlx::Error> {
    let mut series_ids = discovered_series_ids.iter().cloned().collect::<Vec<_>>();
    series_ids.sort();

    let mut renumbered_book_ids = Vec::new();
    for series_id in series_ids {
        let book_rows = sqlx::query(
            r#"SELECT b.ID AS BOOK_ID, b.NAME AS BOOK_NAME, b.NUMBER AS BOOK_NUMBER,
       COALESCE(bm.NUMBER, '') AS METADATA_NUMBER,
       COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS METADATA_NUMBER_SORT,
       COALESCE(bm.NUMBER_LOCK, 0) AS METADATA_NUMBER_LOCK,
       COALESCE(bm.NUMBER_SORT_LOCK, 0) AS METADATA_NUMBER_SORT_LOCK
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
WHERE b.SERIES_ID = ?
  AND b.DELETED_DATE IS NULL
ORDER BY b.ID ASC"#,
        )
        .bind(&series_id)
        .fetch_all(pool)
        .await?;

        let mut books = book_rows
            .into_iter()
            .map(|row| PersistedScannedSeriesBookRow {
                book_id: row.get::<String, _>("BOOK_ID"),
                book_name: row.get::<String, _>("BOOK_NAME"),
                book_number: row.get::<i64, _>("BOOK_NUMBER"),
                metadata_number: row.get::<String, _>("METADATA_NUMBER"),
                metadata_number_sort: row.get::<f64, _>("METADATA_NUMBER_SORT"),
                metadata_number_lock: row.get::<bool, _>("METADATA_NUMBER_LOCK"),
                metadata_number_sort_lock: row.get::<bool, _>("METADATA_NUMBER_SORT_LOCK"),
            })
            .collect::<Vec<_>>();
        books.sort_by(|left, right| {
            compare_book_names_kotlin_like(&left.book_name, &right.book_name)
                .then_with(|| left.book_id.cmp(&right.book_id))
        });

        for (index, book) in books.iter().enumerate() {
            let new_number = index as i64 + 1;
            let new_metadata_number = new_number.to_string();
            let new_metadata_number_sort = new_number as f64;

            if book.book_number != new_number {
                sqlx::query(
                    r#"UPDATE BOOK
SET NUMBER = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
                )
                .bind(new_number)
                .bind(&book.book_id)
                .execute(pool)
                .await?;
            }

            let metadata_number_changed =
                !book.metadata_number_lock && book.metadata_number != new_metadata_number;
            let metadata_number_sort_changed = !book.metadata_number_sort_lock
                && (book.metadata_number_sort - new_metadata_number_sort).abs() > f64::EPSILON;
            if metadata_number_changed || metadata_number_sort_changed {
                sqlx::query(
                    r#"UPDATE BOOK_METADATA
SET NUMBER = CASE WHEN NUMBER_LOCK = 0 THEN ? ELSE NUMBER END,
    NUMBER_SORT = CASE WHEN NUMBER_SORT_LOCK = 0 THEN ? ELSE NUMBER_SORT END,
    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE BOOK_ID = ?"#,
                )
                .bind(&new_metadata_number)
                .bind(new_metadata_number_sort)
                .bind(&book.book_id)
                .execute(pool)
                .await?;
                renumbered_book_ids.push(book.book_id.clone());
            }
        }
    }

    Ok(renumbered_book_ids)
}

pub(crate) async fn load_changed_sidecars(
    pool: &SqlitePool,
    library_id: &str,
    scanned_sidecars: &[ScannedSidecarRow],
) -> Result<Vec<String>, String> {
    if scanned_sidecars.is_empty() {
        return Ok(Vec::new());
    }

    let existing_rows = sqlx::query(
        r#"SELECT URL,
       CASE
           WHEN typeof(LAST_MODIFIED_TIME) IN ('integer', 'real') THEN CAST(LAST_MODIFIED_TIME AS INTEGER)
           ELSE unixepoch(LAST_MODIFIED_TIME)
       END AS LAST_MODIFIED_TIME
FROM SIDECAR
WHERE LIBRARY_ID = ?"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("failed to load existing sidecars for '{library_id}': {error}"))?;

    let existing = existing_rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("URL"),
                row.get::<Option<i64>, _>("LAST_MODIFIED_TIME"),
            )
        })
        .collect::<std::collections::HashMap<_, _>>();

    Ok(scanned_sidecars
        .iter()
        .filter(|sidecar| {
            existing.get(&sidecar.url).and_then(|timestamp| *timestamp)
                != Some(sidecar.last_modified_unix_seconds)
        })
        .map(|sidecar| sidecar.url.clone())
        .collect())
}

fn unavailable_scanned_library() -> ScannedLibrary {
    ScannedLibrary {
        root_available: false,
        series_rows: Vec::new(),
        sidecars: Vec::new(),
        book_ids: Vec::new(),
        changed_existing_book_ids: HashSet::new(),
        series_ids_requiring_book_sync: HashSet::new(),
        discovered_series_ids: HashSet::new(),
        discovered_book_ids: HashSet::new(),
    }
}

async fn ensure_series_metadata_seed(
    pool: &SqlitePool,
    series: &ScannedSeriesRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SERIES_ID)
VALUES (?, ?, ?, ?)"#,
    )
    .bind("ONGOING")
    .bind(&series.series_name)
    .bind(&series.series_name)
    .bind(&series.series_id)
    .execute(pool)
    .await?;

    sqlx::query(r#"INSERT OR IGNORE INTO BOOK_METADATA_AGGREGATION (SERIES_ID) VALUES (?)"#)
        .bind(&series.series_id)
        .execute(pool)
        .await?;

    Ok(())
}

async fn ensure_book_metadata_seed(
    pool: &SqlitePool,
    book: &ScannedBookRow,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT OR IGNORE INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID)
SELECT ?, ?, ?, ?
WHERE EXISTS (SELECT 1 FROM BOOK WHERE ID = ? AND DELETED_DATE IS NULL)"#,
    )
    .bind("0")
    .bind(0.0_f64)
    .bind(&book.book_name)
    .bind(&book.book_id)
    .bind(&book.book_id)
    .execute(pool)
    .await?;

    Ok(())
}

async fn load_existing_scanned_books_by_url(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<HashMap<String, ExistingScannedBookRow>, String> {
    let rows = sqlx::query(
        r#"SELECT ID, URL, SERIES_ID, unixepoch(FILE_LAST_MODIFIED) AS FILE_LAST_MODIFIED
FROM BOOK
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load existing BOOK rows for deep scan in '{library_id}': {error}")
    })?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("URL"),
                ExistingScannedBookRow {
                    book_id: row.get::<String, _>("ID"),
                    series_id: row.get::<String, _>("SERIES_ID"),
                    file_last_modified_unix_seconds: row.get::<i64, _>("FILE_LAST_MODIFIED"),
                },
            )
        })
        .collect())
}

async fn load_existing_scanned_series_by_url(
    pool: &SqlitePool,
    library_id: &str,
) -> Result<HashMap<String, ExistingScannedSeriesRow>, String> {
    let rows = sqlx::query(
        r#"SELECT URL, unixepoch(FILE_LAST_MODIFIED) AS FILE_LAST_MODIFIED
FROM SERIES
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        format!("failed to load existing SERIES rows for scan in '{library_id}': {error}")
    })?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("URL"),
                ExistingScannedSeriesRow {
                    file_last_modified_unix_seconds: row.get::<i64, _>("FILE_LAST_MODIFIED"),
                },
            )
        })
        .collect())
}

fn collect_series_directories(
    current: &Path,
    scan_config: &LibraryScanConfig,
    discovered: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if is_hidden_path(current)
        || is_library_path_excluded(current, &scan_config.scan_directory_exclusions)
    {
        return Ok(());
    }

    let entries = fs::read_dir(current)
        .map_err(|error| format!("failed to scan directory '{}': {error}", current.display()))?;

    let mut has_supported_book = false;
    let mut children = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.is_file()
            && !is_hidden_path(path.as_path())
            && is_supported_book_file(path.as_path(), scan_config)
        {
            has_supported_book = true;
        }
        if metadata.is_dir() {
            children.push(path);
        }
    }

    if has_supported_book {
        discovered.push(current.to_path_buf());
    }

    for child in children {
        collect_series_directories(child.as_path(), scan_config, discovered)?;
    }

    Ok(())
}

fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

fn is_supported_book_file(path: &Path, scan_config: &LibraryScanConfig) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "cbz" | "zip" | "cbr" | "rar"
            )
            .then_some(scan_config.scan_cbx)
            .unwrap_or_else(|| {
                matches!(extension.to_ascii_lowercase().as_str(), "pdf")
                    .then_some(scan_config.scan_pdf)
                    .or_else(|| {
                        matches!(extension.to_ascii_lowercase().as_str(), "epub")
                            .then_some(scan_config.scan_epub)
                    })
                    .unwrap_or(false)
            })
        })
}

fn is_library_path_excluded(path: &Path, exclusions: &[String]) -> bool {
    let path_key = path
        .to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase();
    exclusions.iter().any(|entry| {
        let exclusion = entry.replace('\\', "/").to_ascii_lowercase();
        if exclusion.is_empty() {
            return false;
        }

        path_key.contains(&exclusion)
    })
}

fn resolve_oneshot_series_id(
    existing_books_by_url: &HashMap<String, ExistingScannedBookRow>,
    library_root: &Path,
    book_url: &str,
) -> String {
    existing_books_by_url
        .get(&scanner_url_key(library_root, book_url))
        .map(|existing| existing.series_id.clone())
        .unwrap_or_else(|| {
            let resolved_path = resolve_rooted_path(library_root, book_url);
            route_safe_scanner_id("series", resolved_path.as_path())
        })
}

fn scanner_url_key(root: &Path, stored_url: &str) -> String {
    normalize_scanner_path_key(resolve_rooted_path(root, stored_url).as_path())
}

fn normalize_scanner_path_key(path: &Path) -> String {
    let normalized = path.components().collect::<PathBuf>();
    #[cfg(windows)]
    {
        normalized
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
    }
    #[cfg(not(windows))]
    {
        normalized.to_string_lossy().to_string()
    }
}

fn route_safe_scanner_id(prefix: &str, path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    normalize_scanner_path_key(path).hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

fn build_sidecars(
    series_url: &str,
    books: &[ScannedBookRow],
    sidecar_candidates: &[(PathBuf, fs::Metadata)],
    include_series_sidecars: bool,
) -> Vec<ScannedSidecarRow> {
    let mut sidecars = Vec::new();

    'candidate: for (path, metadata) in sidecar_candidates {
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        let file_stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();

        let is_image = matches!(
            path.extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .as_deref(),
            Some("jpg")
                | Some("jpeg")
                | Some("png")
                | Some("tbn")
                | Some("webp")
                | Some("gif")
                | Some("avif")
        );

        if include_series_sidecars && is_image {
            let base = file_stem.to_ascii_lowercase();
            if matches!(
                base.as_str(),
                "cover" | "default" | "folder" | "poster" | "series"
            ) {
                sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: series_url.to_string(),
                    last_modified_unix_seconds: metadata_updated_unix_seconds(metadata),
                    source: ScannedSidecarSource::Series,
                    sidecar_type: ScannedSidecarType::Artwork,
                });
                continue;
            }
        }

        if include_series_sidecars
            && (file_name.eq_ignore_ascii_case("ComicInfo.xml")
                || file_name.eq_ignore_ascii_case("series.json"))
        {
            sidecars.push(ScannedSidecarRow {
                url: path.to_string_lossy().to_string(),
                parent_url: series_url.to_string(),
                last_modified_unix_seconds: metadata_updated_unix_seconds(metadata),
                source: ScannedSidecarSource::Series,
                sidecar_type: ScannedSidecarType::Metadata,
            });
            continue;
        }

        for book in books {
            let expected = format!("{}.xml", book.book_name);
            if file_name.eq_ignore_ascii_case(&expected) {
                sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: book.book_url.clone(),
                    last_modified_unix_seconds: metadata_updated_unix_seconds(metadata),
                    source: ScannedSidecarSource::Book,
                    sidecar_type: ScannedSidecarType::Metadata,
                });
                continue 'candidate;
            }

            if is_image && is_book_artwork_sidecar(file_stem, &book.book_name) {
                sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: book.book_url.clone(),
                    last_modified_unix_seconds: metadata_updated_unix_seconds(metadata),
                    source: ScannedSidecarSource::Book,
                    sidecar_type: ScannedSidecarType::Artwork,
                });
                continue 'candidate;
            }
        }
    }

    sidecars
}

fn is_book_artwork_sidecar(base_name: &str, book_name: &str) -> bool {
    let base_name = base_name.to_ascii_lowercase();
    let book_name = book_name.to_ascii_lowercase();
    if base_name == book_name {
        return true;
    }

    base_name
        .strip_prefix(&format!("{book_name}-"))
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

fn to_unix_seconds(time: Option<std::time::SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

fn metadata_updated_unix_seconds(metadata: &fs::Metadata) -> i64 {
    [metadata.created().ok(), metadata.modified().ok()]
        .into_iter()
        .map(to_unix_seconds)
        .max()
        .unwrap_or(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::{connect_test_pool, setup};
    use sha2::{Digest, Sha256};
    use sqlx::Row;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after unix epoch")
                .as_nanos()
        ))
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher
            .finalize()
            .iter()
            .map(|value| format!("{value:02x}"))
            .collect::<String>()
    }

    #[tokio::test]
    async fn persist_scanned_library_restores_matching_deleted_book_state() {
        let db_path = temp_path("scanner-restore-book-db");
        let library_root = temp_path("scanner-restore-book-root");
        std::fs::create_dir_all(library_root.join("books"))
            .expect("restore-book root should be created");
        let new_bytes = b"restored-book-content";
        std::fs::write(library_root.join("books/restored.cbz"), new_bytes)
            .expect("restore-book source file should be written");
        let expected_hash = sha256_hex(new_bytes);

        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("restore-book db should open");
        setup::bootstrap_pool(&pool)
            .await
            .expect("restore-book db should bootstrap");

        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("library row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?)"#,
        )
        .bind("series-1")
        .bind(0_i64)
        .bind("Series One")
        .bind("series/series-one")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("series row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SERIES_ID)
VALUES (?, ?, ?, ?)"#,
        )
        .bind("ONGOING")
        .bind("Series One")
        .bind("Series One")
        .bind("series-1")
        .execute(&pool)
        .await
        .expect("series metadata row should be inserted");

        sqlx::query(
            r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)"#,
        )
        .bind("book-deleted")
        .bind(0_i64)
        .bind("legacy-title")
        .bind("books/legacy.cbz")
        .bind("series-1")
        .bind(new_bytes.len() as i64)
        .bind(1_i64)
        .bind("library-1")
        .bind(&expected_hash)
        .execute(&pool)
        .await
        .expect("deleted book row should be inserted");
        sqlx::query(
            r#"INSERT INTO BOOK_METADATA (TITLE, TITLE_LOCK, SUMMARY, SUMMARY_LOCK, NUMBER, NUMBER_LOCK, NUMBER_SORT, NUMBER_SORT_LOCK, ISBN, ISBN_LOCK, AUTHORS_LOCK, TAGS_LOCK, LINKS_LOCK, BOOK_ID)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("Imported Legacy Title")
        .bind(false)
        .bind("legacy summary")
        .bind(true)
        .bind("7")
        .bind(true)
        .bind(7.0_f64)
        .bind(true)
        .bind("isbn-legacy")
        .bind(true)
        .bind(false)
        .bind(false)
        .bind(false)
        .bind("book-deleted")
        .execute(&pool)
        .await
        .expect("deleted book metadata row should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_AUTHOR (BOOK_ID, NAME, ROLE) VALUES (?, ?, ?)")
            .bind("book-deleted")
            .bind("Jane Doe")
            .bind("writer")
            .execute(&pool)
            .await
            .expect("deleted book author should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_TAG (BOOK_ID, TAG) VALUES (?, ?)")
            .bind("book-deleted")
            .bind("restored")
            .execute(&pool)
            .await
            .expect("deleted book tag should be inserted");
        sqlx::query("INSERT INTO BOOK_METADATA_LINK (BOOK_ID, LABEL, URL) VALUES (?, ?, ?)")
            .bind("book-deleted")
            .bind("wiki")
            .bind("https://example.invalid")
            .execute(&pool)
            .await
            .expect("deleted book link should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA (BOOK_ID, STATUS, MEDIA_TYPE, PAGE_COUNT, COMMENT, EPUB_DIVINA_COMPATIBLE, EPUB_IS_KEPUB) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("book-deleted")
        .bind("READY")
        .bind("application/zip")
        .bind(12_i64)
        .bind("legacy media")
        .bind(true)
        .bind(false)
        .execute(&pool)
        .await
        .expect("deleted media row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?)",
        )
        .bind("legacy.cbz")
        .bind("book-deleted")
        .bind("application/zip")
        .bind(new_bytes.len() as i64)
        .execute(&pool)
        .await
        .expect("deleted media file row should be inserted");
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (FILE_NAME, MEDIA_TYPE, NUMBER, BOOK_ID, FILE_HASH, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("page-1.jpg")
        .bind("image/jpeg")
        .bind(0_i64)
        .bind("book-deleted")
        .bind("page-hash")
        .bind(111_i64)
        .execute(&pool)
        .await
        .expect("deleted media page row should be inserted");
        sqlx::query(
            "INSERT INTO THUMBNAIL_BOOK (ID, BOOK_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("thumbnail-book-1")
        .bind("book-deleted")
        .bind("GENERATED")
        .bind(true)
        .bind(vec![1_u8, 2_u8, 3_u8])
        .bind("image/jpeg")
        .bind(3_i64)
        .bind(10_i64)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("deleted book thumbnail row should be inserted");
        sqlx::query(
            "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES, AGE_RESTRICTION_ALLOW_ONLY) VALUES (?, ?, ?, ?, ?)",
        )
            .bind("user-1")
            .bind("user-1@example.com")
            .bind("password")
            .bind(true)
            .bind(false)
            .execute(&pool)
            .await
            .expect("restore-book user row should be inserted");
        sqlx::query(
            "INSERT INTO READ_PROGRESS (BOOK_ID, USER_ID, PAGE, COMPLETED, READ_DATE, LAST_MODIFIED_DATE, CREATED_DATE, DEVICE_ID, DEVICE_NAME) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("book-deleted")
        .bind("user-1")
        .bind(12_i64)
        .bind(true)
        .bind("2024-01-01 00:00:00")
        .bind("2024-01-01 00:00:00")
        .bind("2024-01-01 00:00:00")
        .bind("device-1")
        .bind("Device 1")
        .execute(&pool)
        .await
        .expect("deleted read progress row should be inserted");
        sqlx::query(
            "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED) VALUES (?, ?, ?, ?, ?)",
        )
        .bind("readlist-1")
        .bind("ReadList 1")
        .bind(1_i64)
        .bind("")
        .bind(true)
        .execute(&pool)
        .await
        .expect("readlist row should be inserted");
        sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
            .bind("readlist-1")
            .bind("book-deleted")
            .bind(0_i64)
            .execute(&pool)
            .await
            .expect("readlist book row should be inserted");

        let scanned = ScannedLibrary {
            root_available: true,
            series_rows: vec![ScannedSeriesRow {
                series_id: "series-1".to_string(),
                series_name: "Series One".to_string(),
                series_url: "series/series-one".to_string(),
                series_last_modified_unix_seconds: 1,
                oneshot: false,
                books: vec![ScannedBookRow {
                    book_id: "book-new".to_string(),
                    book_name: "restored".to_string(),
                    book_url: "books/restored.cbz".to_string(),
                    file_name: "restored.cbz".to_string(),
                    file_size: new_bytes.len() as i64,
                    file_last_modified_unix_seconds: 1,
                    oneshot: false,
                }],
            }],
            sidecars: Vec::new(),
            book_ids: vec!["book-new".to_string()],
            changed_existing_book_ids: HashSet::new(),
            series_ids_requiring_book_sync: HashSet::new(),
            discovered_series_ids: HashSet::from(["series-1".to_string()]),
            discovered_book_ids: HashSet::from(["book-new".to_string()]),
        };

        let outcome = persist_scanned_library(&pool, "library-1", &scanned)
            .await
            .expect("restore-book scan persist should succeed");
        assert!(outcome.changed_series_ids.iter().any(|id| id == "series-1"));
        assert_eq!(
            outcome.book_metadata_refreshes,
            vec![BookMetadataRefreshRequest {
                book_id: "book-new".to_string(),
                series_id: "series-1".to_string(),
                capabilities: vec!["TITLE".to_string()],
            }],
        );

        let verify_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("restore-book verify db should open");
        let new_book =
            sqlx::query("SELECT FILE_HASH, SERIES_ID, DELETED_DATE FROM BOOK WHERE ID = ? LIMIT 1")
                .bind("book-new")
                .fetch_one(&verify_pool)
                .await
                .expect("restored book should exist");
        assert_eq!(new_book.get::<String, _>("FILE_HASH"), expected_hash);
        assert_eq!(new_book.get::<String, _>("SERIES_ID"), "series-1");
        assert!(new_book.get::<Option<String>, _>("DELETED_DATE").is_none());

        let new_metadata = sqlx::query(
            "SELECT TITLE, SUMMARY, ISBN, NUMBER, NUMBER_SORT FROM BOOK_METADATA WHERE BOOK_ID = ? LIMIT 1",
        )
        .bind("book-new")
        .fetch_one(&verify_pool)
        .await
        .expect("restored book metadata should exist");
        assert_eq!(new_metadata.get::<String, _>("TITLE"), "restored");
        assert_eq!(new_metadata.get::<String, _>("SUMMARY"), "legacy summary");
        assert_eq!(new_metadata.get::<String, _>("ISBN"), "isbn-legacy");
        assert_eq!(new_metadata.get::<String, _>("NUMBER"), "7");
        assert_eq!(new_metadata.get::<f64, _>("NUMBER_SORT"), 7.0_f64);

        let author_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID = ? AND NAME = ? AND ROLE = ?",
        )
        .bind("book-new")
        .bind("Jane Doe")
        .bind("writer")
        .fetch_one(&verify_pool)
        .await
        .expect("restored author count should be queryable");
        assert_eq!(author_count, 1);

        let media = sqlx::query(
            "SELECT STATUS, MEDIA_TYPE, PAGE_COUNT, COMMENT FROM MEDIA WHERE BOOK_ID = ? LIMIT 1",
        )
        .bind("book-new")
        .fetch_one(&verify_pool)
        .await
        .expect("restored media should exist");
        assert_eq!(media.get::<String, _>("STATUS"), "READY");
        assert_eq!(media.get::<String, _>("MEDIA_TYPE"), "application/zip");
        assert_eq!(media.get::<i64, _>("PAGE_COUNT"), 12_i64);
        assert_eq!(media.get::<String, _>("COMMENT"), "legacy media");

        let thumbnail_book_id =
            sqlx::query("SELECT BOOK_ID FROM THUMBNAIL_BOOK WHERE ID = ? LIMIT 1")
                .bind("thumbnail-book-1")
                .fetch_one(&verify_pool)
                .await
                .expect("restored thumbnail should exist")
                .get::<String, _>("BOOK_ID");
        assert_eq!(thumbnail_book_id, "book-new");

        let read_progress_book_id =
            sqlx::query("SELECT BOOK_ID FROM READ_PROGRESS WHERE USER_ID = ? LIMIT 1")
                .bind("user-1")
                .fetch_one(&verify_pool)
                .await
                .expect("restored read progress should exist")
                .get::<String, _>("BOOK_ID");
        assert_eq!(read_progress_book_id, "book-new");

        let readlist_book_id =
            sqlx::query("SELECT BOOK_ID FROM READLIST_BOOK WHERE READLIST_ID = ? LIMIT 1")
                .bind("readlist-1")
                .fetch_one(&verify_pool)
                .await
                .expect("restored readlist mapping should exist")
                .get::<String, _>("BOOK_ID");
        assert_eq!(readlist_book_id, "book-new");

        let deleted_old = sqlx::query("SELECT 1 FROM BOOK WHERE ID = ? LIMIT 1")
            .bind("book-deleted")
            .fetch_optional(&verify_pool)
            .await
            .expect("deleted legacy book lookup should succeed");
        assert!(deleted_old.is_none());

        verify_pool.close().await;
        let _ = std::fs::remove_dir_all(library_root);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn persist_scanned_library_restores_matching_deleted_series_state() {
        let db_path = temp_path("scanner-restore-series-db");
        let library_root = temp_path("scanner-restore-series-root");
        std::fs::create_dir_all(library_root.join("restored-series"))
            .expect("restore-series root should be created");
        let book_one = b"restored-series-book-one";
        let book_two = b"restored-series-book-two";
        std::fs::write(library_root.join("restored-series/001.cbz"), book_one)
            .expect("restore-series first file should be written");
        std::fs::write(library_root.join("restored-series/002.cbz"), book_two)
            .expect("restore-series second file should be written");
        let hash_one = sha256_hex(book_one);
        let hash_two = sha256_hex(book_two);

        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("restore-series db should open");
        setup::bootstrap_pool(&pool)
            .await
            .expect("restore-series db should bootstrap");

        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind("library-1")
            .bind("Library 1")
            .bind(library_root.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("restore-series library row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, DELETED_DATE)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, CURRENT_TIMESTAMP)"#,
        )
        .bind("series-deleted")
        .bind(0_i64)
        .bind("Old Deleted Series")
        .bind("deleted-series")
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("deleted series row should be inserted");
        sqlx::query(
            r#"INSERT INTO SERIES_METADATA (STATUS, STATUS_LOCK, TITLE, TITLE_LOCK, TITLE_SORT, TITLE_SORT_LOCK, SUMMARY, SUMMARY_LOCK, SERIES_ID)
VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind("ENDED")
        .bind(true)
        .bind("Locked Legacy Series")
        .bind(true)
        .bind("Locked Legacy Series")
        .bind(true)
        .bind("legacy series summary")
        .bind(true)
        .bind("series-deleted")
        .execute(&pool)
        .await
        .expect("deleted series metadata should be inserted");
        sqlx::query("INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT) VALUES (?, ?, ?, ?)")
            .bind("collection-1")
            .bind("Collection 1")
            .bind(false)
            .bind(1_i64)
            .execute(&pool)
            .await
            .expect("collection row should be inserted");
        sqlx::query(
            "INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)",
        )
        .bind("collection-1")
        .bind("series-deleted")
        .bind(0_i64)
        .execute(&pool)
        .await
        .expect("collection membership should be inserted");
        sqlx::query(
            "INSERT INTO THUMBNAIL_SERIES (ID, SERIES_ID, TYPE, SELECTED, THUMBNAIL, MEDIA_TYPE, FILE_SIZE, WIDTH, HEIGHT) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind("thumbnail-series-1")
        .bind("series-deleted")
        .bind("USER_UPLOADED")
        .bind(true)
        .bind(vec![9_u8, 9_u8, 9_u8])
        .bind("image/jpeg")
        .bind(3_i64)
        .bind(10_i64)
        .bind(10_i64)
        .execute(&pool)
        .await
        .expect("series thumbnail should be inserted");

        for (book_id, name, url, hash, size, number) in [
            (
                "book-deleted-1",
                "legacy-one",
                "deleted/one.cbz",
                hash_one.as_str(),
                book_one.len() as i64,
                1_i64,
            ),
            (
                "book-deleted-2",
                "legacy-two",
                "deleted/two.cbz",
                hash_two.as_str(),
                book_two.len() as i64,
                2_i64,
            ),
        ] {
            sqlx::query(
                r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID, FILE_HASH, DELETED_DATE)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)"#,
            )
            .bind(book_id)
            .bind(0_i64)
            .bind(name)
            .bind(url)
            .bind("series-deleted")
            .bind(size)
            .bind(number)
            .bind("library-1")
            .bind(hash)
            .execute(&pool)
            .await
            .expect("deleted series book should be inserted");
            sqlx::query(
                r#"INSERT INTO BOOK_METADATA (TITLE, NUMBER, NUMBER_SORT, BOOK_ID)
VALUES (?, ?, ?, ?)"#,
            )
            .bind(name)
            .bind(number.to_string())
            .bind(number as f64)
            .bind(book_id)
            .execute(&pool)
            .await
            .expect("deleted series book metadata should be inserted");
        }

        let scanned = ScannedLibrary {
            root_available: true,
            series_rows: vec![ScannedSeriesRow {
                series_id: "series-new".to_string(),
                series_name: "Folder Name".to_string(),
                series_url: "restored-series".to_string(),
                series_last_modified_unix_seconds: 1,
                oneshot: false,
                books: vec![
                    ScannedBookRow {
                        book_id: "book-new-1".to_string(),
                        book_name: "001".to_string(),
                        book_url: "restored-series/001.cbz".to_string(),
                        file_name: "001.cbz".to_string(),
                        file_size: book_one.len() as i64,
                        file_last_modified_unix_seconds: 1,
                        oneshot: false,
                    },
                    ScannedBookRow {
                        book_id: "book-new-2".to_string(),
                        book_name: "002".to_string(),
                        book_url: "restored-series/002.cbz".to_string(),
                        file_name: "002.cbz".to_string(),
                        file_size: book_two.len() as i64,
                        file_last_modified_unix_seconds: 1,
                        oneshot: false,
                    },
                ],
            }],
            sidecars: Vec::new(),
            book_ids: vec!["book-new-1".to_string(), "book-new-2".to_string()],
            changed_existing_book_ids: HashSet::new(),
            series_ids_requiring_book_sync: HashSet::new(),
            discovered_series_ids: HashSet::from(["series-new".to_string()]),
            discovered_book_ids: HashSet::from([
                "book-new-1".to_string(),
                "book-new-2".to_string(),
            ]),
        };

        let outcome = persist_scanned_library(&pool, "library-1", &scanned)
            .await
            .expect("restore-series scan persist should succeed");
        assert!(
            outcome
                .changed_series_ids
                .iter()
                .any(|id| id == "series-new")
        );

        let verify_pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("restore-series verify db should open");
        let restored_series =
            sqlx::query("SELECT TITLE FROM SERIES_METADATA WHERE SERIES_ID = ? LIMIT 1")
                .bind("series-new")
                .fetch_one(&verify_pool)
                .await
                .expect("restored series metadata should exist");
        assert_eq!(
            restored_series.get::<String, _>("TITLE"),
            "Locked Legacy Series"
        );

        let collection_series_id =
            sqlx::query("SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? LIMIT 1")
                .bind("collection-1")
                .fetch_one(&verify_pool)
                .await
                .expect("restored collection membership should exist")
                .get::<String, _>("SERIES_ID");
        assert_eq!(collection_series_id, "series-new");

        let thumbnail_series_id =
            sqlx::query("SELECT SERIES_ID FROM THUMBNAIL_SERIES WHERE ID = ? LIMIT 1")
                .bind("thumbnail-series-1")
                .fetch_one(&verify_pool)
                .await
                .expect("restored series thumbnail should exist")
                .get::<String, _>("SERIES_ID");
        assert_eq!(thumbnail_series_id, "series-new");

        let deleted_series = sqlx::query("SELECT 1 FROM SERIES WHERE ID = ? LIMIT 1")
            .bind("series-deleted")
            .fetch_optional(&verify_pool)
            .await
            .expect("deleted legacy series lookup should succeed");
        assert!(deleted_series.is_none());

        verify_pool.close().await;
        let _ = std::fs::remove_dir_all(library_root);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn load_changed_sidecars_treats_legacy_integer_seconds_as_unchanged() {
        let db_path = temp_path("scanner-sidecar-legacy-integer-seconds");

        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("sidecar db should open");
        setup::bootstrap_pool(&pool)
            .await
            .expect("sidecar db should bootstrap");

        sqlx::query(
            "INSERT INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) VALUES (?, ?, ?, ?)",
        )
        .bind("/library/Series-A/Book-001.xml")
        .bind("/library/Series-A/Book-001.cbz")
        .bind(0_i64)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("legacy integer sidecar row should be inserted");

        let changed = load_changed_sidecars(
            &pool,
            "library-1",
            &[ScannedSidecarRow {
                url: "/library/Series-A/Book-001.xml".to_string(),
                parent_url: "/library/Series-A/Book-001.cbz".to_string(),
                last_modified_unix_seconds: 0,
                source: ScannedSidecarSource::Book,
                sidecar_type: ScannedSidecarType::Metadata,
            }],
        )
        .await
        .expect("legacy integer sidecar timestamps should load without false positives");

        assert!(
            changed.is_empty(),
            "matching legacy integer sidecar timestamps must not be treated as changed",
        );

        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn scanner_url_key_normalizes_windows_and_relative_path_shapes() {
        #[cfg(windows)]
        {
            let root = PathBuf::from("C:/library");
            assert_eq!(
                scanner_url_key(root.as_path(), "oneshots/existing.cbz"),
                scanner_url_key(root.as_path(), "C:\\library\\oneshots\\existing.cbz"),
                "scanner url keys should match regardless of separator style so oneshot restoration stays platform-neutral",
            );
        }

        #[cfg(not(windows))]
        {
            let root = PathBuf::from("/library");
            assert_eq!(
                scanner_url_key(root.as_path(), "oneshots/existing.cbz"),
                "/library/oneshots/existing.cbz",
            );
        }
    }
}
