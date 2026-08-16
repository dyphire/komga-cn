use anyhow::Context;
use std::collections::HashSet;

use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_domain::discovery::compare_book_names;
use sqlx::{Row, SqlitePool};

use crate::persisted_paths::resolve_stored_path;
use crate::sql::task_queue::{DELETE_BOOK_DEPENDENCY_SQL, DELETE_SERIES_DEPENDENCY_SQL};

use super::scan_models::{
    BookMetadataRefreshRequest, InsertedBookCandidate, InsertedSeriesCandidate,
    PersistScannedLibraryOutcome, PersistedScannedSeriesBookRow, ScannedBookRow, ScannedLibrary,
    ScannedSeriesRow, ScannedSidecarRow,
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

        let discovered_series_ids = scanned.discovered_series_ids.clone();
        let active_book_ids = soft_delete_missing_scan_rows(
            pool,
            &library_id,
            &discovered_series_ids,
            &scanned.discovered_book_ids,
            &mut runtime_events,
            &mut changed_series_ids,
        )
        .await?;

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
       OR oneshot != ?
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
            .context("failed to update SERIES rows")?
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
                    .context("failed to insert SERIES rows")?
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
                    anyhow::anyhow!(error).context(format!(
                        "failed to ensure SERIES metadata rows for '{}': ",
                        series.series_id
                    ))
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
       OR oneshot != ?
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
                    .context("failed to update BOOK rows")?
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
                            .context("failed to insert BOOK rows")?
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
                        anyhow::anyhow!(error).context(format!(
                            "failed to ensure BOOK metadata rows for '{}': ",
                            book.book_id
                        ))
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
                    .context("failed to update MEDIA_FILE rows")?
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
                        .context("failed to insert MEDIA_FILE rows")?;
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
                anyhow::anyhow!(error).context(format!(
                    "failed to mark MEDIA rows outdated after deep scan for '{book_id}': "
                ))
            })?;
        }

        persist_scanned_sidecars(pool, &library_id, &scanned.sidecars).await?;

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
        restore_deleted_scan_matches(
            pool,
            &library_id,
            &inserted_series,
            &inserted_books,
            &mut changed_series_ids,
            &mut book_metadata_refreshes,
        )
        .await?;

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
    runtime_events: &mut RuntimeSseEventBuffer,
    changed_series_ids: &mut HashSet<String>,
) -> anyhow::Result<HashSet<String>> {
    let existing_series = sqlx::query(
        r#"SELECT ID
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
    })?;
    let existing_books = sqlx::query(
        r#"SELECT ID, SERIES_ID
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
        )
    })
    .collect::<Vec<_>>();
    let active_book_ids = existing_books
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
        if discovered_book_ids.contains(book_id) || !missing_series_id_set.contains(series_id) {
            continue;
        }
        soft_delete_missing_book(pool, book_id).await?;
        record_book_runtime_sse_event(
            runtime_events,
            book_id,
            series_id,
            library_id,
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
            anyhow::anyhow!(error).context(format!(
                "failed to soft-delete missing SERIES '{series_id}': "
            ))
        })?;
        record_series_runtime_sse_event(
            runtime_events,
            series_id,
            library_id,
            RuntimeSseMutationKind::Changed,
        );
    }

    for (book_id, series_id) in &existing_books {
        if discovered_book_ids.contains(book_id) || missing_series_id_set.contains(series_id) {
            continue;
        }
        soft_delete_missing_book(pool, book_id).await?;
        record_book_runtime_sse_event(
            runtime_events,
            book_id,
            series_id,
            library_id,
            RuntimeSseMutationKind::Changed,
        );
        changed_series_ids.insert(series_id.clone());
    }

    Ok(active_book_ids)
}

async fn soft_delete_missing_book(pool: &SqlitePool, book_id: &str) -> anyhow::Result<()> {
    sqlx::query(
        r#"UPDATE BOOK
SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP
WHERE ID = ?"#,
    )
    .bind(book_id)
    .execute(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!("failed to soft-delete missing BOOK '{book_id}'"))
    })?;

    Ok(())
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
        for sql in DELETE_BOOK_DEPENDENCY_SQL {
            sqlx::query(*sql)
                .bind(&deleted_book_id)
                .execute(pool)
                .await
                .map_err(|error| {
                    anyhow::anyhow!(error)
                        .context("failed to delete restored legacy series book dependencies: ")
                })?;
        }
    }
    sqlx::query("DELETE FROM BOOK WHERE SERIES_ID = ?")
        .bind(deleted_series_id)
        .execute(pool)
        .await
        .context("failed to delete restored legacy series BOOK rows: ")?;
    for sql in DELETE_SERIES_DEPENDENCY_SQL {
        sqlx::query(*sql)
            .bind(deleted_series_id)
            .execute(pool)
            .await
            .map_err(|error| {
                anyhow::anyhow!(error)
                    .context("failed to delete restored legacy series dependencies: ")
            })?;
    }
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

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use komga_application::runtime_sse::RuntimeSseEventStore;

    use super::*;
    use crate::sqlite::{connect_test_pool, setup};

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
        setup::bootstrap_pool(&pool)
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
            "library 'missing-library' does not exist"
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
        setup::bootstrap_pool(&pool)
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
}
