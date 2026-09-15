use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_domain::discovery::compare_book_names;
use sqlx::{Row, SqlitePool};

use komga_infrastructure_base::stored_paths::resolve_stored_path;
use komga_infrastructure_discovery::{delete_book_dependency_rows, delete_series_dependency_rows};

use super::scan_models::{
    BookMetadataRefreshRequest, InsertedBookCandidate, InsertedSeriesCandidate,
    PersistScannedLibraryOutcome, PersistedScannedSeriesBookRow, ScannedLibrary,
    ScannedSidecarRow,
};
use super::scan_restore::{try_restore_deleted_books, try_restore_deleted_series};
use super::scan_sse::{
    RuntimeSseEventBuffer, RuntimeSseMutationKind, emit_scanned_library_runtime_sse_events,
    record_book_runtime_sse_event, record_series_runtime_sse_event,
};

pub(super) struct ScannedLibraryPersistence<'a> {
    pool: &'a SqlitePool,
    runtime_events: &'a dyn RuntimeSseEventSink,
    library_id: &'a str,
    scanned: &'a ScannedLibrary,
}

pub(super) struct ScannedLibraryPersistenceResult {
    pub(super) changed_sidecar_urls: Vec<String>,
    pub(super) renumbered_book_ids: Vec<String>,
    pub(super) changed_series_ids: Vec<String>,
    pub(super) book_metadata_refreshes: Vec<BookMetadataRefreshRequest>,
    pub(super) should_empty_trash: bool,
}

impl<'a> ScannedLibraryPersistence<'a> {
    pub(super) fn new(
        pool: &'a SqlitePool,
        runtime_events: &'a dyn RuntimeSseEventSink,
        library_id: &'a str,
        scanned: &'a ScannedLibrary,
    ) -> Self {
        Self {
            pool,
            runtime_events,
            library_id,
            scanned,
        }
    }

    pub(super) async fn execute(self) -> anyhow::Result<ScannedLibraryPersistenceResult> {
        let changed_sidecar_urls =
            load_changed_sidecars(self.pool, self.library_id, &self.scanned.sidecars).await?;
        let outcome = persist_scanned_library(self.pool, self.library_id, self.scanned).await?;
        let should_empty_trash = library_empty_trash_after_scan(self.pool, self.library_id).await?;
        emit_scanned_library_runtime_sse_events(self.runtime_events, self.library_id, &outcome);

        Ok(ScannedLibraryPersistenceResult {
            changed_sidecar_urls,
            renumbered_book_ids: outcome.renumbered_book_ids,
            changed_series_ids: outcome.changed_series_ids,
            book_metadata_refreshes: outcome.book_metadata_refreshes,
            should_empty_trash,
        })
    }
}

async fn library_empty_trash_after_scan(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<bool> {
    let row = sqlx::query(
        r#"SELECT EMPTY_TRASH_AFTER_SCAN
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load empty-trash-after-scan flag for '{library_id}': "
        ))
    })?;

    let Some(row) = row else {
        return Err(anyhow::anyhow!(format!(
            "library '{library_id}' does not exist"
        )));
    };

    Ok(row.get::<bool, _>("EMPTY_TRASH_AFTER_SCAN"))
}

async fn persist_scanned_library(
    pool: &SqlitePool,
    library_id: &str,
    scanned: &ScannedLibrary,
) -> anyhow::Result<PersistScannedLibraryOutcome> {
    persist_scanned_library_inner(pool, library_id, scanned)
        .await
        .context("failed to persist scanned library changes")
}

async fn persist_scanned_library_inner(
    pool: &SqlitePool,
    library_id: &str,
    scanned: &ScannedLibrary,
) -> anyhow::Result<PersistScannedLibraryOutcome> {
    let library_id = library_id.to_string();
    let outcome: PersistScannedLibraryOutcome = 'outcome: {
        let mut book_metadata_refreshes = Vec::<BookMetadataRefreshRequest>::new();
        let mut runtime_events = RuntimeSseEventBuffer::default();
        let mut changed_series_ids = HashSet::<String>::new();
        let mut inserted_books = Vec::<InsertedBookCandidate>::new();
        let mut inserted_series = Vec::<InsertedSeriesCandidate>::new();
        let library_row = sqlx::query(
            r#"SELECT UNAVAILABLE_DATE
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
        )
        .bind(&library_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to load library availability state for '{library_id}': "
            ))
        })?;
        let Some(library_row) = library_row else {
            return Err(anyhow::anyhow!(format!(
                "library '{library_id}' does not exist"
            )));
        };
        let library_was_unavailable = library_row
            .get::<Option<String>, _>("UNAVAILABLE_DATE")
            .is_some();

        if !scanned.root_available {
            let updated = sqlx::query(
                r#"UPDATE LIBRARY
SET UNAVAILABLE_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
            )
            .bind(&library_id)
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to mark library unavailable for '{library_id}': "
                ))
            })?
            .rows_affected();
            if updated == 0 {
                return Err(anyhow::anyhow!(format!(
                    "library '{library_id}' does not exist"
                )));
            }
            break 'outcome PersistScannedLibraryOutcome {
                renumbered_book_ids: Vec::new(),
                library_changed: !library_was_unavailable,
                changed_series_ids: Vec::new(),
                book_metadata_refreshes: Vec::new(),
                runtime_events: runtime_events.events,
            };
        }

        if library_was_unavailable {
            let updated = sqlx::query(
                r#"UPDATE LIBRARY
SET UNAVAILABLE_DATE = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
            )
            .bind(&library_id)
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to clear library unavailable marker for '{library_id}': "
                ))
            })?
            .rows_affected();
            if updated == 0 {
                return Err(anyhow::anyhow!(format!(
                    "library '{library_id}' does not exist"
                )));
            }
        }

        // Mark mtime-changed books' MEDIA as OUTDATED up front, before any other
        // write in this autocommit pipeline: if a later step fails (partial
        // commit), the re-analysis trigger is already persisted and the next
        // successful scan's follow-up planner will pick these books up again.
        if !scanned.changed_existing_book_ids.is_empty() {
            let outdated_book_ids: Vec<&String> =
                scanned.changed_existing_book_ids.iter().collect();
            for chunk in outdated_book_ids.chunks(500) {
                let mut media_outdated_query =
                    sqlx::QueryBuilder::new("UPDATE MEDIA SET STATUS = 'OUTDATED' WHERE BOOK_ID IN (");
                let mut separated = media_outdated_query.separated(", ");
                for book_id in chunk {
                    separated.push_bind(book_id);
                }
                separated.push_unseparated(")");
                media_outdated_query
                    .build()
                    .execute(pool)
                    .await
                    .context("failed to mark MEDIA rows outdated after deep scan")?;
            }
        }

        let discovered_series_ids = scanned.discovered_series_ids.clone();
        let mut series_with_deleted_books = HashSet::new();
        if !scanned.failed_directories.is_empty() {
            tracing::warn!(
                library_id,
                failed_directories = scanned.failed_directories.len(),
                "scan: treating rows under failed directories as unknown (excluded from deletion)"
            );
        }
        let active_book_ids = soft_delete_missing_scan_rows(
            pool,
            &library_id,
            &discovered_series_ids,
            &scanned.discovered_book_ids,
            &scanned.failed_directories,
            &mut runtime_events,
            &mut changed_series_ids,
            &mut series_with_deleted_books,
        )
        .await?;

        let existing_series_ids: HashSet<String> =
            sqlx::query_scalar("SELECT ID FROM SERIES WHERE LIBRARY_ID = ?")
                .bind(&library_id)
                .fetch_all(pool)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(error).context(format!(
                        "failed to load existing SERIES IDs for '{library_id}': "
                    ))
                })?
                .into_iter()
                .collect();

        let existing_book_ids: HashSet<String> =
            sqlx::query_scalar("SELECT ID FROM BOOK WHERE LIBRARY_ID = ?")
                .bind(&library_id)
                .fetch_all(pool)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(error).context(format!(
                        "failed to load existing BOOK IDs for '{library_id}': "
                    ))
                })?
                .into_iter()
                .collect();

        let mut series_updated_in_main_loop = HashSet::new();
        for series in &scanned.series_rows {
            // One transaction per series: a committed batch always carries its
            // series/book rows plus metadata seeds, an aborted batch leaves
            // nothing behind (failed scan keeps already-committed batches).
            let mut tx = pool
                .begin()
                .await
                .context("failed to begin per-series persistence transaction")?;
            let mut inserted_in_series = Vec::<InsertedBookCandidate>::new();
            let series_changed = sqlx::query(
                r#"INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?)
ON CONFLICT(ID) DO UPDATE SET
  FILE_LAST_MODIFIED = excluded.FILE_LAST_MODIFIED,
  NAME = excluded.NAME,
  URL = excluded.URL,
  LIBRARY_ID = excluded.LIBRARY_ID,
  oneshot = excluded.oneshot,
  LAST_MODIFIED_DATE = CURRENT_TIMESTAMP,
  DELETED_DATE = NULL
  WHERE (unixepoch(FILE_LAST_MODIFIED) != unixepoch(excluded.FILE_LAST_MODIFIED)
         OR NAME != excluded.NAME
         OR URL != excluded.URL
         OR LIBRARY_ID != excluded.LIBRARY_ID
         OR oneshot != excluded.oneshot
         OR DELETED_DATE IS NOT NULL)"#,
            )
            .bind(&series.series_id)
            .bind(series.series_last_modified_unix_seconds)
            .bind(&series.series_name)
            .bind(&series.series_url)
            .bind(&library_id)
            .bind(series.oneshot)
            .execute(&mut *tx)
            .await
            .context("failed to upsert SERIES rows")?
            .rows_affected();

            if series_changed != 0 {
                series_updated_in_main_loop.insert(series.series_id.clone());
                changed_series_ids.insert(series.series_id.clone());
            }

            let series_inserted = series_changed != 0
                && !existing_series_ids.contains(&series.series_id);
            if series_inserted {
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

            let sync_books = series_inserted
                || scanned
                    .series_ids_requiring_book_sync
                    .contains(&series.series_id);
            let created_date_unix_seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("system clock is before UNIX_EPOCH")?
                .as_secs() as i64;
            for book in &series.books {
                if sync_books || !active_book_ids.contains(&book.book_id) {
                    let book_changed = sqlx::query(
                        r#"INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE,
 LIBRARY_ID, oneshot, CREATED_DATE)
VALUES (?, datetime(?, 'unixepoch'), ?, ?, ?, ?, ?, ?, datetime(?, 'unixepoch'))
ON CONFLICT(ID) DO UPDATE SET
  FILE_LAST_MODIFIED = excluded.FILE_LAST_MODIFIED,
  NAME = excluded.NAME,
  URL = excluded.URL,
  SERIES_ID = excluded.SERIES_ID,
  FILE_SIZE = excluded.FILE_SIZE,
  LIBRARY_ID = excluded.LIBRARY_ID,
  oneshot = excluded.oneshot,
  LAST_MODIFIED_DATE = CURRENT_TIMESTAMP,
  DELETED_DATE = NULL
  WHERE (unixepoch(FILE_LAST_MODIFIED) != unixepoch(excluded.FILE_LAST_MODIFIED)
         OR URL != excluded.URL
         OR SERIES_ID != excluded.SERIES_ID
         OR FILE_SIZE != excluded.FILE_SIZE
         OR LIBRARY_ID != excluded.LIBRARY_ID
         OR oneshot != excluded.oneshot
         OR DELETED_DATE IS NOT NULL)"#,
                    )
                    .bind(&book.book_id)
                    .bind(book.file_last_modified_unix_seconds)
                    .bind(&book.book_name)
                    .bind(&book.book_url)
                    .bind(&series.series_id)
                    .bind(book.file_size)
                    .bind(&library_id)
                    .bind(book.oneshot)
                    .bind(created_date_unix_seconds)
                    .execute(&mut *tx)
                    .await
                    .context("failed to upsert BOOK rows")?
                    .rows_affected();

                    if book_changed != 0 {
                        let book_inserted =
                            !existing_book_ids.contains(&book.book_id);
                        if book_inserted {
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
            }

            // Seed metadata rows for this series inside the same transaction so
            // a committed batch always carries its metadata (INSERT OR IGNORE
            // keeps this idempotent across scans).
            sqlx::query(
                "INSERT OR IGNORE INTO SERIES_METADATA (STATUS, TITLE, TITLE_SORT, SERIES_ID) VALUES (?, ?, ?, ?)",
            )
            .bind("ONGOING")
            .bind(&series.series_name)
            .bind(&series.series_name)
            .bind(&series.series_id)
            .execute(&mut *tx)
            .await
            .context("failed to insert SERIES_METADATA rows")?;

            sqlx::query(
                "INSERT OR IGNORE INTO BOOK_METADATA_AGGREGATION (SERIES_ID) VALUES (?)",
            )
            .bind(&series.series_id)
            .execute(&mut *tx)
            .await
            .context("failed to insert BOOK_METADATA_AGGREGATION rows")?;

            if !series.books.is_empty() {
                for book_chunk in series.books.chunks(500) {
                    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                        "INSERT OR IGNORE INTO BOOK_METADATA (NUMBER, NUMBER_SORT, TITLE, BOOK_ID) ",
                    );
                    query
                        .push_values(book_chunk.iter(), |mut binder, book| {
                            binder
                                .push_bind("0")
                                .push_bind(0.0_f64)
                                .push_bind(&book.book_name)
                                .push_bind(&book.book_id);
                        })
                        .build()
                        .execute(&mut *tx)
                        .await
                        .context("failed to insert BOOK_METADATA rows")?;
                }
            }

            tx.commit()
                .await
                .context("failed to commit per-series persistence transaction")?;

            if !inserted_in_series.is_empty()
                && let Some(series_candidate) = inserted_series
                    .iter_mut()
                    .find(|candidate| candidate.series_id == series.series_id)
            {
                series_candidate.books.extend(inserted_in_series.clone());
            }
        }

        persist_scanned_sidecars(pool, &library_id, &scanned.sidecars).await?;

        let refresh_candidates = series_with_deleted_books
            .iter()
            .filter(|series_id| !series_updated_in_main_loop.contains(*series_id))
            .collect::<Vec<_>>();
        for refresh_chunk in refresh_candidates.chunks(500) {
            let mut refresh = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
                "UPDATE SERIES SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID IN (",
            );
            let mut separated = refresh.separated(",");
            for series_id in refresh_chunk {
                separated.push_bind(series_id);
            }
            separated.push_unseparated(")");
            refresh
                .build()
                .execute(pool)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(error).context(
                        "failed to refresh LAST_MODIFIED_DATE for series with deleted books",
                    )
                })?;
        }

        restore_deleted_scan_matches(
            pool,
            &library_id,
            &inserted_series,
            &inserted_books,
            &mut changed_series_ids,
            &mut book_metadata_refreshes,
        )
        .await?;

        sqlx::query(
            r#"UPDATE SERIES
SET BOOK_COUNT = (SELECT COUNT(*)
                  FROM BOOK
                  WHERE BOOK.SERIES_ID = SERIES.ID)
WHERE LIBRARY_ID = ?"#,
        )
        .bind(&library_id)
        .execute(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to refresh series book counts after scan for '{library_id}': "
            ))
        })?;

        let renumbered_book_ids = resort_scanned_series_books(pool, &discovered_series_ids)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to apply Kotlin-like series numbering after scan for '{library_id}'"
                ))
            })?;

        break 'outcome PersistScannedLibraryOutcome {
            renumbered_book_ids,
            library_changed: library_was_unavailable,
            changed_series_ids: changed_series_ids.into_iter().collect(),
            book_metadata_refreshes,
            runtime_events: runtime_events.events,
        };
    };
    Ok(outcome)
}

async fn resort_scanned_series_books(
    pool: &SqlitePool,
    discovered_series_ids: &HashSet<String>,
) -> Result<Vec<String>, sqlx::Error> {
    let mut series_ids = discovered_series_ids.iter().cloned().collect::<Vec<_>>();
    series_ids.sort();

    let mut renumbered_book_ids = Vec::new();
    if series_ids.is_empty() {
        return Ok(renumbered_book_ids);
    }

    let mut books_by_series_id = HashMap::<String, Vec<PersistedScannedSeriesBookRow>>::new();
    for series_chunk in series_ids.chunks(500) {
        let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            r#"SELECT b.SERIES_ID AS SERIES_ID, b.ID AS BOOK_ID, b.NAME AS BOOK_NAME, b.NUMBER AS BOOK_NUMBER,
       COALESCE(bm.NUMBER, '') AS METADATA_NUMBER,
       COALESCE(bm.NUMBER_SORT, CAST(0 AS REAL)) AS METADATA_NUMBER_SORT,
       COALESCE(bm.NUMBER_LOCK, 0) AS METADATA_NUMBER_LOCK,
       COALESCE(bm.NUMBER_SORT_LOCK, 0) AS METADATA_NUMBER_SORT_LOCK
FROM BOOK b
LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID
WHERE b.SERIES_ID IN ("#,
        );
        let mut separated = query.separated(",");
        for series_id in series_chunk {
            separated.push_bind(series_id);
        }
        separated.push_unseparated(")");
        let rows = query.build().fetch_all(pool).await?;
        for row in rows {
            let series_id = row.get::<String, _>("SERIES_ID");
            books_by_series_id
                .entry(series_id)
                .or_default()
                .push(PersistedScannedSeriesBookRow {
                    book_id: row.get::<String, _>("BOOK_ID"),
                    book_name: row.get::<String, _>("BOOK_NAME"),
                    book_number: row.get::<i64, _>("BOOK_NUMBER"),
                    metadata_number: row.get::<String, _>("METADATA_NUMBER"),
                    metadata_number_sort: row.get::<f64, _>("METADATA_NUMBER_SORT"),
                    metadata_number_lock: row.get::<bool, _>("METADATA_NUMBER_LOCK"),
                    metadata_number_sort_lock: row.get::<bool, _>("METADATA_NUMBER_SORT_LOCK"),
                });
        }
    }

    for series_id in series_ids {
        let Some(books) = books_by_series_id.get_mut(&series_id) else {
            continue;
        };
        books.sort_by(|left, right| {
            compare_book_names(&left.book_name, &right.book_name)
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
                let metadata_number = if book.metadata_number_lock {
                    book.metadata_number.clone()
                } else {
                    new_metadata_number
                };
                let metadata_number_sort = if book.metadata_number_sort_lock {
                    book.metadata_number_sort
                } else {
                    new_metadata_number_sort
                };

                sqlx::query(
                    r#"UPDATE BOOK_METADATA
SET NUMBER = ?,
    NUMBER_SORT = ?,
    LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE BOOK_ID = ?"#,
                )
                .bind(&metadata_number)
                .bind(metadata_number_sort)
                .bind(&book.book_id)
                .execute(pool)
                .await?;
                renumbered_book_ids.push(book.book_id.clone());
            }
        }
    }

    Ok(renumbered_book_ids)
}

async fn soft_delete_missing_scan_rows(
    pool: &SqlitePool,
    library_id: &str,
    discovered_series_ids: &HashSet<String>,
    discovered_book_ids: &HashSet<String>,
    failed_directories: &[std::path::PathBuf],
    runtime_events: &mut RuntimeSseEventBuffer,
    changed_series_ids: &mut HashSet<String>,
    series_with_deleted_books: &mut HashSet<String>,
) -> anyhow::Result<HashSet<String>> {
    let existing_series = sqlx::query(
        r#"SELECT ID, URL
FROM SERIES
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to query existing SERIES rows for '{library_id}': "
        ))
    })?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("ID"),
            row.get::<String, _>("URL"),
        )
    })
    .collect::<Vec<_>>();
    let existing_books = sqlx::query(
        r#"SELECT ID, SERIES_ID, URL
FROM BOOK
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to query existing BOOK rows for '{library_id}': "
        ))
    })?
    .into_iter()
    .map(|row| {
        (
            row.get::<String, _>("ID"),
            row.get::<String, _>("SERIES_ID"),
            row.get::<String, _>("URL"),
        )
    })
    .collect::<Vec<_>>();

    // Rows located under a directory that failed to read during discovery are
    // in an unknown state (the file may still exist): they must be excluded
    // from the "missing" set so a transient WebDAV/network hiccup can never
    // soft-delete real books. The next scan re-reads those directories.
    let is_protected = |url: &str| {
        let url_path = std::path::Path::new(url);
        failed_directories
            .iter()
            .any(|failed| url_path.starts_with(failed))
    };
    let protected_series_ids = existing_series
        .iter()
        .filter(|(_, url)| is_protected(url))
        .map(|(id, _)| id.clone())
        .collect::<HashSet<_>>();
    let protected_book_ids = existing_books
        .iter()
        .filter(|(_, _, url)| is_protected(url))
        .map(|(id, _, _)| id.clone())
        .collect::<HashSet<_>>();

    let known_present_series_ids = discovered_series_ids
        .union(&protected_series_ids)
        .cloned()
        .collect::<HashSet<_>>();
    let known_present_book_ids = discovered_book_ids
        .union(&protected_book_ids)
        .cloned()
        .collect::<HashSet<_>>();

    let active_book_ids = existing_books
        .iter()
        .map(|(book_id, _, _)| book_id.clone())
        .collect::<HashSet<_>>();
    let missing_series_ids = existing_series
        .into_iter()
        .map(|(id, _)| id)
        .filter(|series_id| !known_present_series_ids.contains(series_id))
        .collect::<Vec<_>>();
    let missing_series_id_set = missing_series_ids.iter().cloned().collect::<HashSet<_>>();

    let orphaned_book_rows = existing_books
        .iter()
        .filter(|(book_id, series_id, _)| {
            !known_present_book_ids.contains(book_id) && missing_series_id_set.contains(series_id)
        })
        .collect::<Vec<_>>();
    for orphaned_chunk in orphaned_book_rows.chunks(500) {
        let mut delete = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID IN (",
        );
        let mut separated = delete.separated(",");
        for (book_id, _, _) in orphaned_chunk {
            separated.push_bind(book_id);
        }
        separated.push_unseparated(")");
        delete
            .build()
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context("failed to bulk soft-delete orphaned BOOK rows")
            })?;
    }
    for (book_id, series_id, _) in orphaned_book_rows {
        record_book_runtime_sse_event(
            runtime_events,
            book_id,
            series_id,
            library_id,
            RuntimeSseMutationKind::Changed,
        );
        changed_series_ids.insert(series_id.clone());
    }

    for missing_chunk in missing_series_ids.chunks(500) {
        let mut delete = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "UPDATE SERIES SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID IN (",
        );
        let mut separated = delete.separated(",");
        for series_id in missing_chunk {
            separated.push_bind(series_id);
        }
        separated.push_unseparated(")");
        delete
            .build()
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context("failed to bulk soft-delete missing SERIES rows")
            })?;
    }
    for series_id in &missing_series_ids {
        record_series_runtime_sse_event(
            runtime_events,
            series_id,
            library_id,
            RuntimeSseMutationKind::Changed,
        );
    }

    let missing_book_rows = existing_books
        .iter()
        .filter(|(book_id, series_id, _)| {
            !known_present_book_ids.contains(book_id) && !missing_series_id_set.contains(series_id)
        })
        .collect::<Vec<_>>();
    for missing_chunk in missing_book_rows.chunks(500) {
        let mut delete = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "UPDATE BOOK SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID IN (",
        );
        let mut separated = delete.separated(",");
        for (book_id, _, _) in missing_chunk {
            separated.push_bind(book_id);
        }
        separated.push_unseparated(")");
        delete
            .build()
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context("failed to bulk soft-delete missing BOOK rows")
            })?;
    }
    for (book_id, series_id, _) in missing_book_rows {
        record_book_runtime_sse_event(
            runtime_events,
            book_id,
            series_id,
            library_id,
            RuntimeSseMutationKind::Changed,
        );
        changed_series_ids.insert(series_id.clone());
        series_with_deleted_books.insert(series_id.clone());
    }

    Ok(active_book_ids)
}

async fn persist_scanned_sidecars(
    pool: &SqlitePool,
    library_id: &str,
    sidecars: &[ScannedSidecarRow],
) -> anyhow::Result<()> {
    for sidecar in sidecars {
        let sidecar_updated = sqlx::query(
            r#"UPDATE SIDECAR
SET PARENT_URL = ?, LAST_MODIFIED_TIME = datetime(?, 'unixepoch')
WHERE URL = ?
  AND LIBRARY_ID = ?"#,
        )
        .bind(&sidecar.parent_url)
        .bind(sidecar.last_modified_unix_seconds)
        .bind(&sidecar.url)
        .bind(library_id)
        .execute(pool)
        .await
        .context("failed to update SIDECAR rows")?
        .rows_affected();

        if sidecar_updated == 0 {
            sqlx::query(
                r#"INSERT OR IGNORE INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID)
VALUES (?, ?, datetime(?, 'unixepoch'), ?)"#,
            )
            .bind(&sidecar.url)
            .bind(&sidecar.parent_url)
            .bind(sidecar.last_modified_unix_seconds)
            .bind(library_id)
            .execute(pool)
            .await
            .context("failed to insert SIDECAR rows")?;
        }
    }

    let scanned_sidecar_urls = sidecars
        .iter()
        .map(|sidecar| sidecar.url.clone())
        .collect::<HashSet<_>>();
    let existing_sidecar_urls = sqlx::query(r#"SELECT URL FROM SIDECAR WHERE LIBRARY_ID = ?"#)
        .bind(library_id)
        .fetch_all(pool)
        .await
        .context("failed to load SIDECAR rows for cleanup")?;
    for row in existing_sidecar_urls {
        let url = row.get::<String, _>("URL");
        if scanned_sidecar_urls.contains(&url) {
            continue;
        }
        sqlx::query(r#"DELETE FROM SIDECAR WHERE LIBRARY_ID = ? AND URL = ?"#)
            .bind(library_id)
            .bind(&url)
            .execute(pool)
            .await
            .context("failed to delete stale SIDECAR row")?;
    }

    Ok(())
}

async fn restore_deleted_scan_matches(
    pool: &SqlitePool,
    library_id: &str,
    inserted_series: &[InsertedSeriesCandidate],
    inserted_books: &[InsertedBookCandidate],
    changed_series_ids: &mut HashSet<String>,
    book_metadata_refreshes: &mut Vec<BookMetadataRefreshRequest>,
) -> anyhow::Result<()> {
    let library_root = resolve_stored_path(
        sqlx::query("SELECT ROOT FROM LIBRARY WHERE ID = ? LIMIT 1")
            .bind(library_id)
            .fetch_one(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to resolve library root for restore in '{library_id}': "
                ))
            })?
            .get::<String, _>("ROOT")
            .as_str(),
    );
    let restored_series_matches =
        try_restore_deleted_series(pool, library_root.as_path(), inserted_series).await?;
    for restored in &restored_series_matches {
        changed_series_ids.insert(restored.inserted_series_id.clone());
    }
    let restored_books =
        try_restore_deleted_books(pool, library_root.as_path(), inserted_books).await?;
    changed_series_ids.extend(restored_books.series_ids);
    book_metadata_refreshes.extend(restored_books.book_metadata_refreshes);
    for restored in &restored_series_matches {
        changed_series_ids.insert(restored.inserted_series_id.clone());
        delete_restored_legacy_series(pool, &restored.deleted_series_id).await?;
    }

    Ok(())
}

async fn delete_restored_legacy_series(
    pool: &SqlitePool,
    deleted_series_id: &str,
) -> anyhow::Result<()> {
    let deleted_book_ids = sqlx::query("SELECT ID FROM BOOK WHERE SERIES_ID = ? ORDER BY ID ASC")
        .bind(deleted_series_id)
        .fetch_all(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error)
                .context("failed to load restored legacy series books for cleanup: ")
        })?;
    for deleted_book_row in deleted_book_ids {
        let deleted_book_id = deleted_book_row.get::<String, _>("ID");
        delete_book_dependency_rows(pool, &deleted_book_id)
            .await
            .context("failed to delete restored legacy series book dependencies")?;
    }
    sqlx::query("DELETE FROM BOOK WHERE SERIES_ID = ?")
        .bind(deleted_series_id)
        .execute(pool)
        .await
        .context("failed to delete restored legacy series BOOK rows: ")?;
    delete_series_dependency_rows(pool, deleted_series_id)
        .await
        .context("failed to delete restored legacy series dependencies")?;
    sqlx::query("DELETE FROM SERIES WHERE ID = ?")
        .bind(deleted_series_id)
        .execute(pool)
        .await
        .context("failed to delete restored legacy SERIES row")?;

    Ok(())
}

async fn load_changed_sidecars(
    pool: &SqlitePool,
    library_id: &str,
    scanned_sidecars: &[ScannedSidecarRow],
) -> anyhow::Result<Vec<String>> {
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
    .map_err(|error| anyhow::anyhow!(error).context( format!("failed to load existing sidecars for '{library_id}'")))?;

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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use komga_application::runtime_sse::RuntimeSseEventStore;

    use super::*;
    use komga_infrastructure_base::sqlite::{connect_test_pool, schema};

    fn temp_db_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-scan-persist-{case_id}-{nanos}.sqlite"))
    }

    #[tokio::test]
    async fn scanned_library_persistence_rejects_missing_library_row() {
        let db_path = temp_db_path("missing-library");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");

        let scanned = ScannedLibrary {
            root_available: true,
            series_rows: Vec::new(),
            sidecars: Vec::new(),
            book_ids: Vec::new(),
            changed_existing_book_ids: HashSet::new(),
            series_ids_requiring_book_sync: HashSet::new(),
            discovered_series_ids: HashSet::new(),
            discovered_book_ids: HashSet::new(),
            failed_directories: Vec::new(),
        };
        let runtime_events = RuntimeSseEventStore::default();

        let error = match ScannedLibraryPersistence::new(
            &pool,
            &runtime_events,
            "missing-library",
            &scanned,
        )
        .execute()
        .await
        {
            Ok(_) => panic!("scan persistence should reject a missing library row"),
            Err(error) => error,
        };

        assert_eq!(
            error.to_string(),
            "failed to persist scanned library changes"
        );
        assert!(
            format!("{error:#}").contains("library 'missing-library' does not exist"),
            "error chain should retain the missing-library cause: {error:#}"
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn library_empty_trash_after_scan_rejects_missing_library_row() {
        let db_path = temp_db_path("missing-empty-trash-library");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");

        let error = library_empty_trash_after_scan(&pool, "missing-library")
            .await
            .expect_err("empty-trash flag lookup should reject a missing library row");

        assert_eq!(
            error.to_string(),
            "library 'missing-library' does not exist"
        );

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn soft_delete_skips_rows_under_failed_directories() {
        let db_path = temp_db_path("failed-dirs");
        let pool = connect_test_pool(db_path.as_path(), 1)
            .await
            .expect("temporary sqlite db should open");
        schema::bootstrap_pool(&pool)
            .await
            .expect("temporary sqlite db should bootstrap main schema");

        let base = std::env::temp_dir().join("komga-rust-failed-dirs-fixture");
        let protected_dir = base.join("seriesA");
        let normal_dir = base.join("seriesB");
        let library_id = "lib-1";

        sqlx::query("INSERT INTO LIBRARY (ID, NAME, ROOT) VALUES (?, ?, ?)")
            .bind(library_id)
            .bind("Fixture")
            .bind(base.to_string_lossy().to_string())
            .execute(&pool)
            .await
            .expect("insert fixture library");

        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, datetime('now'), ?, ?, ?, 0)",
        )
        .bind("series-a")
        .bind("Series A")
        .bind(protected_dir.to_string_lossy().to_string())
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("insert protected series");
        sqlx::query(
            "INSERT INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) VALUES (?, datetime('now'), ?, ?, ?, 0)",
        )
        .bind("series-b")
        .bind("Series B")
        .bind(normal_dir.to_string_lossy().to_string())
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("insert normal series");
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID, oneshot, CREATED_DATE) VALUES (?, datetime('now'), ?, ?, ?, 0, ?, 0, datetime('now'))",
        )
        .bind("book-a1")
        .bind("1")
        .bind(protected_dir.join("1.cbz").to_string_lossy().to_string())
        .bind("series-a")
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("insert protected book");
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, LIBRARY_ID, oneshot, CREATED_DATE) VALUES (?, datetime('now'), ?, ?, ?, 0, ?, 0, datetime('now'))",
        )
        .bind("book-b1")
        .bind("1")
        .bind(normal_dir.join("1.cbz").to_string_lossy().to_string())
        .bind("series-b")
        .bind(library_id)
        .execute(&pool)
        .await
        .expect("insert normal book");

        let mut runtime_events = RuntimeSseEventBuffer::default();
        let mut changed_series_ids = HashSet::new();
        let mut series_with_deleted_books = HashSet::new();
        let active_book_ids = soft_delete_missing_scan_rows(
            &pool,
            library_id,
            &HashSet::new(),
            &HashSet::new(),
            &[protected_dir],
            &mut runtime_events,
            &mut changed_series_ids,
            &mut series_with_deleted_books,
        )
        .await
        .expect("soft delete should not fail");

        async fn series_deleted(pool: &SqlitePool, id: &str) -> Option<String> {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT DELETED_DATE FROM SERIES WHERE ID = ?",
            )
            .bind(id)
            .fetch_one(pool)
            .await
            .ok()
            .flatten()
        }
        async fn book_deleted(pool: &SqlitePool, id: &str) -> Option<String> {
            sqlx::query_scalar::<_, Option<String>>("SELECT DELETED_DATE FROM BOOK WHERE ID = ?")
                .bind(id)
                .fetch_one(pool)
                .await
                .ok()
                .flatten()
        }

        assert!(
            series_deleted(&pool, "series-a").await.is_none(),
            "series under failed directory must not be soft-deleted"
        );
        assert!(
            series_deleted(&pool, "series-b").await.is_some(),
            "series outside failed directories must be soft-deleted"
        );
        assert!(
            book_deleted(&pool, "book-a1").await.is_none(),
            "book under failed directory must not be soft-deleted"
        );
        assert!(
            book_deleted(&pool, "book-b1").await.is_some(),
            "book outside failed directories must be soft-deleted"
        );
        assert!(active_book_ids.contains("book-a1"));
        assert!(active_book_ids.contains("book-b1"));

        pool.close().await;
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_dir_all(base);
    }
}
