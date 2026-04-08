use std::collections::{HashMap, HashSet, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use sqlx::{Row, SqlitePool};

use crate::sqlite::connect_pool;

#[derive(Clone, Debug)]
pub struct LibraryScanConfig {
    pub root: String,
    pub scan_cbx: bool,
    pub scan_pdf: bool,
    pub scan_epub: bool,
    pub scan_force_modified_time: bool,
    pub oneshots_directory: Option<String>,
    pub scan_directory_exclusions: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct ScannedLibrary {
    pub root_available: bool,
    pub series_rows: Vec<ScannedSeriesRow>,
    pub sidecars: Vec<ScannedSidecarRow>,
    pub book_ids: Vec<String>,
    pub changed_existing_book_ids: HashSet<String>,
    pub discovered_series_ids: HashSet<String>,
    pub discovered_book_ids: HashSet<String>,
}

#[derive(Clone, Debug)]
struct ExistingScannedBookRow {
    book_id: String,
    series_id: String,
    file_last_modified_unix_seconds: i64,
}

#[derive(Clone, Debug)]
struct ExistingScannedSeriesRow {
    file_last_modified_unix_seconds: i64,
}

#[derive(Clone, Debug)]
pub struct ScannedSeriesRow {
    pub series_id: String,
    pub series_name: String,
    pub series_url: String,
    pub series_last_modified_unix_seconds: i64,
    pub oneshot: bool,
    pub books: Vec<ScannedBookRow>,
}

#[derive(Clone, Debug)]
pub struct ScannedBookRow {
    pub book_id: String,
    pub book_name: String,
    pub book_url: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_last_modified_unix_seconds: i64,
    pub oneshot: bool,
}

#[derive(Clone, Debug)]
pub struct ScannedSidecarRow {
    pub url: String,
    pub parent_url: String,
    pub last_modified_unix_seconds: i64,
    pub source: ScannedSidecarSource,
    pub sidecar_type: ScannedSidecarType,
}

#[derive(Clone, Copy, Debug)]
pub enum ScannedSidecarSource {
    Series,
    Book,
}

#[derive(Clone, Copy, Debug)]
pub enum ScannedSidecarType {
    Metadata,
    Artwork,
}

pub fn load_library_scan_config(
    database_file: &Path,
    library_id: &str,
) -> Result<Option<LibraryScanConfig>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();
    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let row = sqlx::query(
                "SELECT ROOT, SCAN_CBX, SCAN_PDF, SCAN_EPUB, SCAN_FORCE_MODIFIED_TIME, ONESHOTS_DIRECTORY \
                 FROM LIBRARY \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| format!("failed to load library root: {error}"))?;

            let Some(row) = row else {
                return Ok(None);
            };

            let exclusions = sqlx::query(
                "SELECT EXCLUSION \
                 FROM LIBRARY_EXCLUSIONS \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to load library exclusions for '{library_id}': {error}")
            })?
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
        })
    })
}

pub fn scan_library(
    database_file: &Path,
    library_id: &str,
    deep_scan: bool,
) -> Result<ScannedLibrary, String> {
    let Some(scan_config) = load_library_scan_config(database_file, library_id)? else {
        return Ok(unavailable_scanned_library());
    };

    let existing_books_by_url = load_existing_scanned_books_by_url(database_file, library_id)?;
    let existing_series_by_url = load_existing_scanned_series_by_url(database_file, library_id)?;
    let oneshots_directory: Option<String> = scan_config
        .oneshots_directory
        .as_ref()
        .map(|value| value.to_ascii_lowercase());

    let root = PathBuf::from(&scan_config.root);
    if !root.exists() {
        return Ok(unavailable_scanned_library());
    }

    let mut discovered = Vec::new();
    collect_series_directories(
        root.as_path(),
        root.as_path(),
        &scan_config,
        &mut discovered,
    )?;

    let mut sidecars = Vec::new();
    let mut series_rows = Vec::new();
    let mut book_ids = Vec::new();
    let mut changed_existing_book_ids = HashSet::new();
    let mut changed_book_candidates_by_series_id = HashMap::<String, Vec<String>>::new();
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

            if is_supported_book_file(path.as_path(), &scan_config) {
                let book_id = route_safe_scanner_id("book", path.as_path());
                let book_url = path.to_string_lossy().to_string();
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

                if let Some(existing) = existing_books_by_url.get(&book_url)
                    && existing.file_last_modified_unix_seconds != file_last_modified_unix_seconds
                {
                    let candidate_series_id = if series_is_oneshot {
                        resolve_oneshot_series_id(&existing_books_by_url, &book_url)
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
        let series_changed = existing_series_by_url
            .get(&series_url)
            .is_some_and(|existing| {
                existing.file_last_modified_unix_seconds != series_last_modified_unix_seconds
            });
        if deep_scan || series_changed {
            changed_existing_book_ids.extend(changed_book_candidates);
        }

        for book in &books {
            discovered_book_ids.insert(book.book_id.clone());
        }

        if series_is_oneshot {
            for book in &books {
                let series_id = resolve_oneshot_series_id(&existing_books_by_url, &book.book_url);
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
        discovered_series_ids.insert(series_id.clone());
        let series_name = series_dir
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string();

        sidecars.extend(build_series_sidecars(
            &series_url,
            &books,
            &sidecar_candidates,
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
        discovered_series_ids,
        discovered_book_ids,
    })
}

pub fn library_empty_trash_after_scan(
    database_file: &Path,
    library_id: &str,
) -> Result<bool, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();
    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let value = sqlx::query(
                "SELECT EMPTY_TRASH_AFTER_SCAN \
                 FROM LIBRARY \
                 WHERE ID = ? \
                 LIMIT 1",
            )
            .bind(&library_id)
            .fetch_optional(&pool)
            .await
            .map_err(|error| {
                format!("failed to load empty-trash-after-scan flag for '{library_id}': {error}")
            })?
            .map(|row| row.get::<bool, _>("EMPTY_TRASH_AFTER_SCAN"))
            .unwrap_or(false);

            Ok(value)
        })
    })
}

pub fn persist_scanned_library(
    database_file: &Path,
    library_id: &str,
    scanned: &ScannedLibrary,
) -> Result<(), String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();
    let scanned = scanned.clone();

    run_database_query(database_file, move |pool| {
        Box::pin(async move {
            if !scanned.root_available {
                sqlx::query(
                    "UPDATE LIBRARY \
                     SET UNAVAILABLE_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                     WHERE ID = ?",
                )
                .bind(&library_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to mark library unavailable for '{library_id}': {error}"))?;
                return Ok(());
            }

            sqlx::query(
                "UPDATE LIBRARY \
                 SET UNAVAILABLE_DATE = NULL, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                 WHERE ID = ?",
            )
            .bind(&library_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!("failed to clear library unavailable marker for '{library_id}': {error}")
            })?;

            let discovered_series_ids = scanned.discovered_series_ids.clone();
            let discovered_book_ids = scanned.discovered_book_ids.clone();

            for series in &scanned.series_rows {
                let series_updated = sqlx::query(
                    "UPDATE SERIES \
                     SET FILE_LAST_MODIFIED = ?, NAME = ?, URL = ?, LIBRARY_ID = ?, oneshot = ?, \
                         LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, DELETED_DATE = NULL \
                     WHERE ID = ?",
                )
                .bind(series.series_last_modified_unix_seconds)
                .bind(&series.series_name)
                .bind(&series.series_url)
                .bind(&library_id)
                .bind(series.oneshot)
                .bind(&series.series_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to update SERIES rows: {error}"))?
                .rows_affected();

                if series_updated == 0 {
                    sqlx::query(
                        "INSERT OR IGNORE INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) \
                         VALUES (?, ?, ?, ?, ?, ?)",
                    )
                    .bind(&series.series_id)
                    .bind(series.series_last_modified_unix_seconds)
                    .bind(&series.series_name)
                    .bind(&series.series_url)
                    .bind(&library_id)
                    .bind(series.oneshot)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to insert SERIES rows: {error}"))?;
                }

                for book in &series.books {
                    let book_updated = sqlx::query(
                        "UPDATE BOOK \
                         SET FILE_LAST_MODIFIED = ?, URL = ?, SERIES_ID = ?, FILE_SIZE = ?, \
                             LIBRARY_ID = ?, oneshot = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, DELETED_DATE = NULL \
                         WHERE ID = ?",
                    )
                    .bind(book.file_last_modified_unix_seconds)
                    .bind(&book.book_url)
                    .bind(&series.series_id)
                    .bind(book.file_size)
                    .bind(&library_id)
                    .bind(book.oneshot)
                    .bind(&book.book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to update BOOK rows: {error}"))?
                    .rows_affected();

                    if book_updated == 0 {
                        sqlx::query(
                            "INSERT OR IGNORE INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, \
                                                        LIBRARY_ID, oneshot) \
                             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                        )
                        .bind(&book.book_id)
                        .bind(book.file_last_modified_unix_seconds)
                        .bind(&book.book_name)
                        .bind(&book.book_url)
                        .bind(&series.series_id)
                        .bind(book.file_size)
                        .bind(&library_id)
                        .bind(book.oneshot)
                        .execute(&pool)
                        .await
                        .map_err(|error| format!("failed to insert BOOK rows: {error}"))?;
                    }

                    let media_updated = sqlx::query(
                        "UPDATE MEDIA_FILE \
                         SET FILE_SIZE = ? \
                         WHERE FILE_NAME = ? \
                           AND BOOK_ID = ?",
                    )
                    .bind(book.file_size)
                    .bind(&book.file_name)
                    .bind(&book.book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to update MEDIA_FILE rows: {error}"))?
                    .rows_affected();

                    if media_updated == 0 {
                        sqlx::query(
                            "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE) \
                             VALUES (?, ?, ?)",
                        )
                        .bind(&book.file_name)
                        .bind(&book.book_id)
                        .bind(book.file_size)
                        .execute(&pool)
                        .await
                        .map_err(|error| format!("failed to insert MEDIA_FILE rows: {error}"))?;
                    }
                }
            }

            for book_id in &scanned.changed_existing_book_ids {
                sqlx::query(
                    "UPDATE MEDIA \
                     SET STATUS = 'OUTDATED' \
                     WHERE BOOK_ID = ?",
                )
                .bind(book_id)
                .execute(&pool)
                .await
                .map_err(|error| {
                    format!(
                        "failed to mark MEDIA rows outdated after deep scan for '{book_id}': {error}"
                    )
                })?;
            }

            for sidecar in &scanned.sidecars {
                let sidecar_updated = sqlx::query(
                    "UPDATE SIDECAR \
                     SET PARENT_URL = ?, LAST_MODIFIED_TIME = ? \
                     WHERE URL = ? \
                       AND LIBRARY_ID = ?",
                )
                .bind(&sidecar.parent_url)
                .bind(sidecar.last_modified_unix_seconds)
                .bind(&sidecar.url)
                .bind(&library_id)
                .execute(&pool)
                .await
                .map_err(|error| format!("failed to update SIDECAR rows: {error}"))?
                .rows_affected();

                if sidecar_updated == 0 {
                    sqlx::query(
                        "INSERT OR IGNORE INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) \
                         VALUES (?, ?, ?, ?)",
                    )
                    .bind(&sidecar.url)
                    .bind(&sidecar.parent_url)
                    .bind(sidecar.last_modified_unix_seconds)
                    .bind(&library_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to insert SIDECAR rows: {error}"))?;
                }
            }

            if scanned.root_available {
                let existing_series = sqlx::query(
                    "SELECT ID \
                     FROM SERIES \
                     WHERE LIBRARY_ID = ? \
                       AND DELETED_DATE IS NULL",
                )
                .bind(&library_id)
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    format!("failed to query existing SERIES rows for '{library_id}': {error}")
                })?;
                for row in existing_series {
                    let series_id = row.get::<String, _>("ID");
                    if discovered_series_ids.contains(&series_id) {
                        continue;
                    }
                    sqlx::query(
                        "UPDATE SERIES \
                         SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?",
                    )
                    .bind(&series_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to soft-delete missing SERIES '{series_id}': {error}"))?;
                }

                let existing_books = sqlx::query(
                    "SELECT ID \
                     FROM BOOK \
                     WHERE LIBRARY_ID = ? \
                       AND DELETED_DATE IS NULL",
                )
                .bind(&library_id)
                .fetch_all(&pool)
                .await
                .map_err(|error| {
                    format!("failed to query existing BOOK rows for '{library_id}': {error}")
                })?;
                for row in existing_books {
                    let book_id = row.get::<String, _>("ID");
                    if discovered_book_ids.contains(&book_id) {
                        continue;
                    }
                    sqlx::query(
                        "UPDATE BOOK \
                         SET DELETED_DATE = CURRENT_TIMESTAMP, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP \
                         WHERE ID = ?",
                    )
                    .bind(&book_id)
                    .execute(&pool)
                    .await
                    .map_err(|error| format!("failed to soft-delete missing BOOK '{book_id}': {error}"))?;
                }
            }

            sqlx::query(
                "UPDATE SERIES \
                 SET BOOK_COUNT = (SELECT COUNT(*) \
                                   FROM BOOK \
                                   WHERE BOOK.SERIES_ID = SERIES.ID \
                                     AND BOOK.DELETED_DATE IS NULL) \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .execute(&pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to refresh series book counts after scan for '{library_id}': {error}"
                )
            })?;

            Ok(())
        })
    })
}

pub fn load_changed_sidecars(
    database_file: &Path,
    library_id: &str,
    scanned_sidecars: &[ScannedSidecarRow],
) -> Result<Vec<String>, String> {
    if scanned_sidecars.is_empty() {
        return Ok(Vec::new());
    }

    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();
    let scanned_sidecars = scanned_sidecars.to_vec();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        let scanned_sidecars = scanned_sidecars.clone();
        Box::pin(async move {
            let existing_rows = sqlx::query(
                "SELECT URL, LAST_MODIFIED_TIME \
                 FROM SIDECAR \
                 WHERE LIBRARY_ID = ?",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!("failed to load existing sidecars for '{library_id}': {error}")
            })?;

            let existing = existing_rows
                .into_iter()
                .map(|row| {
                    (
                        row.get::<String, _>("URL"),
                        row.get::<i64, _>("LAST_MODIFIED_TIME"),
                    )
                })
                .collect::<std::collections::HashMap<_, _>>();

            Ok(scanned_sidecars
                .into_iter()
                .filter(|sidecar| {
                    existing
                        .get(&sidecar.url)
                        .is_none_or(|timestamp| *timestamp != sidecar.last_modified_unix_seconds)
                })
                .map(|sidecar| sidecar.url)
                .collect())
        })
    })
}

type BoxFuture<T> = std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, String>> + Send>>;

fn unavailable_scanned_library() -> ScannedLibrary {
    ScannedLibrary {
        root_available: false,
        series_rows: Vec::new(),
        sidecars: Vec::new(),
        book_ids: Vec::new(),
        changed_existing_book_ids: HashSet::new(),
        discovered_series_ids: HashSet::new(),
        discovered_book_ids: HashSet::new(),
    }
}

fn load_existing_scanned_books_by_url(
    database_file: &Path,
    library_id: &str,
) -> Result<HashMap<String, ExistingScannedBookRow>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT ID, URL, SERIES_ID, FILE_LAST_MODIFIED \
                 FROM BOOK \
                 WHERE LIBRARY_ID = ? \
                   AND DELETED_DATE IS NULL",
            )
            .bind(&library_id)
            .fetch_all(&pool)
            .await
            .map_err(|error| {
                format!(
                    "failed to load existing BOOK rows for deep scan in '{library_id}': {error}"
                )
            })?;

            Ok(rows
                .into_iter()
                .map(|row| {
                    (
                        row.get::<String, _>("URL"),
                        ExistingScannedBookRow {
                            book_id: row.get::<String, _>("ID"),
                            series_id: row.get::<String, _>("SERIES_ID"),
                            file_last_modified_unix_seconds: row
                                .get::<i64, _>("FILE_LAST_MODIFIED"),
                        },
                    )
                })
                .collect())
        })
    })
}

fn load_existing_scanned_series_by_url(
    database_file: &Path,
    library_id: &str,
) -> Result<HashMap<String, ExistingScannedSeriesRow>, String> {
    let database_file = database_file.to_path_buf();
    let library_id = library_id.to_string();

    run_database_query(database_file, move |pool| {
        let library_id = library_id.clone();
        Box::pin(async move {
            let rows = sqlx::query(
                "SELECT URL, FILE_LAST_MODIFIED \
                 FROM SERIES \
                 WHERE LIBRARY_ID = ? \
                   AND DELETED_DATE IS NULL",
            )
            .bind(&library_id)
            .fetch_all(&pool)
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
                            file_last_modified_unix_seconds: row
                                .get::<i64, _>("FILE_LAST_MODIFIED"),
                        },
                    )
                })
                .collect())
        })
    })
}

fn collect_series_directories(
    current: &Path,
    root: &Path,
    scan_config: &LibraryScanConfig,
    discovered: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if is_library_path_excluded(current, root, &scan_config.scan_directory_exclusions) {
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
            && is_supported_book_file(path.as_path(), scan_config)
            && !is_library_path_excluded(
                path.as_path(),
                root,
                &scan_config.scan_directory_exclusions,
            )
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
        collect_series_directories(child.as_path(), root, scan_config, discovered)?;
    }

    Ok(())
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

fn is_library_path_excluded(path: &Path, root: &Path, exclusions: &[String]) -> bool {
    let Ok(relative) = path.strip_prefix(root) else {
        return false;
    };

    let relative = relative.to_string_lossy().replace('\\', "/");
    exclusions.iter().any(|entry| {
        let exclusion = entry.trim().replace('\\', "/");
        if exclusion.is_empty() {
            return false;
        }

        relative == exclusion
            || relative.starts_with(&(exclusion.clone() + "/"))
            || relative.contains(&("/".to_string() + &exclusion + "/"))
    })
}

fn resolve_oneshot_series_id(
    existing_books_by_url: &HashMap<String, ExistingScannedBookRow>,
    book_url: &str,
) -> String {
    existing_books_by_url
        .get(book_url)
        .map(|existing| existing.series_id.clone())
        .unwrap_or_else(|| route_safe_scanner_id("series", PathBuf::from(book_url).as_path()))
}

fn route_safe_scanner_id(prefix: &str, path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

fn build_series_sidecars(
    series_url: &str,
    books: &[ScannedBookRow],
    sidecar_candidates: &[(PathBuf, fs::Metadata)],
) -> Vec<ScannedSidecarRow> {
    let mut series_sidecars = Vec::new();

    for (path, metadata) in sidecar_candidates {
        let Some(file_name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        let file_name_lower = file_name.to_ascii_lowercase();
        let is_image = ["jpg", "jpeg", "png", "webp", "gif", "avif"]
            .iter()
            .any(|ext| file_name_lower.ends_with(&format!(".{ext}")));

        if is_image {
            let base = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_ascii_lowercase();
            if matches!(base.as_str(), "cover" | "folder" | "poster" | "series") {
                series_sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: series_url.to_string(),
                    last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                    source: ScannedSidecarSource::Series,
                    sidecar_type: ScannedSidecarType::Artwork,
                });
                continue;
            }
        }

        if file_name.eq_ignore_ascii_case("ComicInfo.xml") {
            series_sidecars.push(ScannedSidecarRow {
                url: path.to_string_lossy().to_string(),
                parent_url: series_url.to_string(),
                last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                source: ScannedSidecarSource::Series,
                sidecar_type: ScannedSidecarType::Metadata,
            });
            continue;
        }

        for book in books {
            let expected = format!("{}.xml", book.book_name);
            if file_name.eq_ignore_ascii_case(&expected) {
                series_sidecars.push(ScannedSidecarRow {
                    url: path.to_string_lossy().to_string(),
                    parent_url: book.book_url.clone(),
                    last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                    source: ScannedSidecarSource::Book,
                    sidecar_type: ScannedSidecarType::Metadata,
                });
                continue;
            }

            if is_image {
                let base = path
                    .file_stem()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default();
                if base.eq_ignore_ascii_case(&book.book_name) {
                    series_sidecars.push(ScannedSidecarRow {
                        url: path.to_string_lossy().to_string(),
                        parent_url: book.book_url.clone(),
                        last_modified_unix_seconds: to_unix_seconds(metadata.modified().ok()),
                        source: ScannedSidecarSource::Book,
                        sidecar_type: ScannedSidecarType::Artwork,
                    });
                }
            }
        }
    }

    series_sidecars
}

fn to_unix_seconds(time: Option<std::time::SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

fn metadata_updated_unix_seconds(metadata: &fs::Metadata) -> i64 {
    to_unix_seconds(metadata.modified().ok())
}

fn run_database_query<T>(
    database_file: PathBuf,
    operation: impl FnOnce(SqlitePool) -> BoxFuture<T> + Send + 'static,
) -> Result<T, String>
where
    T: Send + 'static,
{
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| format!("failed to build task runtime: {error}"))?;

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1)
                .await
                .map_err(|error| format!("failed to open sqlite pool: {error}"))?;
            operation(pool).await
        })
    })
    .join()
    .map_err(|_| "database operation worker thread panicked".to_string())?
}
