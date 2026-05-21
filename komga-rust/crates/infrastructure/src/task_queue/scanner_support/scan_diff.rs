use std::collections::{HashMap, HashSet};
use std::fs;

use sqlx::{Row, SqlitePool};

use crate::persisted_paths::resolve_stored_path;

use super::scan_discovery::{
    build_sidecars, collect_series_directories, is_hidden_path, is_supported_book_file,
    metadata_updated_unix_seconds, resolve_oneshot_series_id, route_safe_scanner_id,
    scanner_url_key,
};
use super::scan_models::*;

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
