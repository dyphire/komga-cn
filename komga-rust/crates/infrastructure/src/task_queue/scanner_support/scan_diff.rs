use anyhow::Context;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::ErrorKind;

use sqlx::{Row, SqlitePool};

use crate::persisted_paths::resolve_stored_path;

use super::scan_discovery::{
    build_sidecars, collect_series_directories, is_hidden_path, is_supported_book_file,
    metadata_updated_unix_seconds, path_file_name_utf8, path_file_stem_utf8,
    resolve_oneshot_series_id, route_safe_scanner_id, scanner_url_key,
};
use super::scan_models::{
    ExistingScannedBookRow, ExistingScannedSeriesRow, LibraryScanConfig, ScannedBookRow,
    ScannedLibrary, ScannedSeriesRow,
};

pub(super) async fn scan_library(
    pool: &SqlitePool,
    library_id: &str,
    deep_scan: bool,
) -> anyhow::Result<ScannedLibrary> {
    let Some(scan_config) = load_library_scan_config(pool, library_id).await? else {
        return Err(anyhow::anyhow!(format!(
            "library '{library_id}' does not exist"
        )));
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

pub(super) fn build_scanned_library(
    scan_config: LibraryScanConfig,
    existing_books_by_url: HashMap<String, ExistingScannedBookRow>,
    existing_series_by_url: HashMap<String, ExistingScannedSeriesRow>,
    deep_scan: bool,
) -> anyhow::Result<ScannedLibrary> {
    let oneshots_directory: Option<String> = scan_config
        .oneshots_directory
        .as_ref()
        .map(|value| value.to_ascii_lowercase());

    let root = resolve_stored_path(&scan_config.root);
    match fs::metadata(&root) {
        Ok(_) => {}
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Ok(unavailable_scanned_library());
        }
        Err(error) => {
            return Err(anyhow::anyhow!(format!(
                "failed to inspect library scan root '{}': {error}",
                root.display()
            )));
        }
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
        let series_dir_metadata = fs::metadata(&series_dir).map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to read series directory metadata for '{}': ",
                series_dir.display()
            ))
        })?;
        let series_dir_last_modified_unix_seconds =
            metadata_updated_unix_seconds(&series_dir_metadata, series_dir.as_path())?;

        let entries = fs::read_dir(&series_dir).map_err(|error| {
            anyhow::anyhow!(error).context(format!(
                "failed to scan series directory '{}': ",
                series_dir.display()
            ))
        })?;

        let mut books = Vec::new();
        let mut changed_book_candidates = Vec::new();
        let mut sidecar_candidates = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to read directory entry in '{}': ",
                    series_dir.display()
                ))
            })?;
            let path = entry.path();

            if is_hidden_path(path.as_path()) {
                continue;
            }

            let metadata = entry.metadata().map_err(|error| {
                anyhow::anyhow!(error).context(format!(
                    "failed to read metadata for '{}': ",
                    path.display()
                ))
            })?;

            if !metadata.is_file() {
                continue;
            }

            if is_supported_book_file(path.as_path(), &scan_config) {
                let book_url = path.to_string_lossy().to_string();
                let book_url_key = scanner_url_key(root.as_path(), &book_url);
                let book_id = existing_books_by_url
                    .get(&book_url_key)
                    .map(|existing| existing.book_id.clone())
                    .unwrap_or_else(|| route_safe_scanner_id("book", path.as_path()));
                let file_last_modified_unix_seconds =
                    metadata_updated_unix_seconds(&metadata, path.as_path())?;
                let book_name = path_file_stem_utf8(path.as_path())?.to_string();
                let file_name = path_file_name_utf8(path.as_path())?.to_string();

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
            )?);
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
        let series_name = path_file_name_utf8(series_dir.as_path())?.to_string();

        sidecars.extend(build_sidecars(
            &series_url,
            &books,
            &sidecar_candidates,
            true,
        )?);

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

pub(super) async fn load_library_scan_config(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<Option<LibraryScanConfig>> {
    let row = sqlx::query(
        r#"SELECT ROOT, SCAN_CBX, SCAN_PDF, SCAN_EPUB, SCAN_FORCE_MODIFIED_TIME, ONESHOTS_DIRECTORY
FROM LIBRARY
WHERE ID = ?
LIMIT 1"#,
    )
    .bind(library_id)
    .fetch_optional(pool)
    .await
    .context("failed to load library root")?;

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
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load library exclusions for '{library_id}': "
        ))
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
}

async fn load_existing_scanned_books_by_url(
    pool: &SqlitePool,
    library_id: &str,
) -> anyhow::Result<HashMap<String, ExistingScannedBookRow>> {
    let rows = sqlx::query(
        r#"SELECT ID, URL, SERIES_ID, oneshot AS ONESHOT, unixepoch(FILE_LAST_MODIFIED) AS FILE_LAST_MODIFIED
FROM BOOK
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| { anyhow::anyhow!(error).context( format!("failed to load existing BOOK rows for deep scan in '{library_id}'"))
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
) -> anyhow::Result<HashMap<String, ExistingScannedSeriesRow>> {
    let rows = sqlx::query(
        r#"SELECT URL, oneshot AS ONESHOT, unixepoch(FILE_LAST_MODIFIED) AS FILE_LAST_MODIFIED
FROM SERIES
WHERE LIBRARY_ID = ?
  AND DELETED_DATE IS NULL"#,
    )
    .bind(library_id)
    .fetch_all(pool)
    .await
    .map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "failed to load existing SERIES rows for scan in '{library_id}': "
        ))
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

#[cfg(all(test, unix))]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn temp_library_path(case_id: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("komga-rust-scan-diff-{case_id}-{nanos}"))
    }

    fn scan_config_for_root(root: &std::path::Path) -> LibraryScanConfig {
        LibraryScanConfig {
            root: root.to_string_lossy().to_string(),
            scan_cbx: true,
            scan_pdf: true,
            scan_epub: true,
            scan_force_modified_time: false,
            oneshots_directory: None,
            scan_directory_exclusions: Vec::new(),
        }
    }

    #[test]
    fn scan_propagates_root_metadata_errors() {
        let root = temp_library_path("root-metadata-error");
        std::fs::create_dir_all(&root).expect("scan fixture root should exist");
        std::fs::write(root.join("blocked"), b"not a directory")
            .expect("blocking root component should be written");
        let scan_root = root.join("blocked/library");

        let error = build_scanned_library(
            scan_config_for_root(scan_root.as_path()),
            HashMap::new(),
            HashMap::new(),
            false,
        )
        .expect_err("scanner root metadata errors should be propagated");

        assert!(
            error
                .to_string()
                .contains("failed to inspect library scan root")
        );

        let _ = std::fs::remove_dir_all(root);
    }
}
