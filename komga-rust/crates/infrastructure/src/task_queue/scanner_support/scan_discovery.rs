use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use crate::persisted_paths::resolve_rooted_path;

use super::scan_models::*;

pub(super) fn collect_series_directories(
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

pub(super) fn is_hidden_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|name| name.starts_with('.'))
}

pub(super) fn is_supported_book_file(path: &Path, scan_config: &LibraryScanConfig) -> bool {
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

pub(super) fn is_library_path_excluded(path: &Path, exclusions: &[String]) -> bool {
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

pub(super) fn resolve_oneshot_series_id(
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

pub(super) fn scanner_url_key(root: &Path, stored_url: &str) -> String {
    normalize_scanner_path_key(resolve_rooted_path(root, stored_url).as_path())
}

pub(super) fn normalize_scanner_path_key(path: &Path) -> String {
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

pub(super) fn route_safe_scanner_id(prefix: &str, path: &Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalize_scanner_path_key(path).hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

pub(super) fn build_sidecars(
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

pub(super) fn is_book_artwork_sidecar(base_name: &str, book_name: &str) -> bool {
    let base_name = base_name.to_ascii_lowercase();
    let book_name = book_name.to_ascii_lowercase();
    if base_name == book_name {
        return true;
    }

    base_name
        .strip_prefix(&format!("{book_name}-"))
        .is_some_and(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
}

pub(super) fn to_unix_seconds(time: Option<std::time::SystemTime>) -> i64 {
    time.and_then(|value| value.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|value| value.as_secs() as i64)
        .unwrap_or(0)
}

pub(super) fn metadata_updated_unix_seconds(metadata: &fs::Metadata) -> i64 {
    [metadata.created().ok(), metadata.modified().ok()]
        .into_iter()
        .map(to_unix_seconds)
        .max()
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

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
