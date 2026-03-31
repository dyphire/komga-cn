use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanBook {
    pub name: String,
    pub path: PathBuf,
    pub file_last_modified: SystemTime,
    pub file_size: u64,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanSeries {
    pub name: String,
    pub path: PathBuf,
    pub file_last_modified: SystemTime,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScanSidecarSource {
    Series,
    Book,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanSidecar {
    pub path: PathBuf,
    pub target_path: PathBuf,
    pub file_last_modified: SystemTime,
    pub sidecar_type: String,
    pub source: ScanSidecarSource,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScannedSeries {
    pub series: ScanSeries,
    pub books: Vec<ScanBook>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ScanResult {
    pub series: Vec<ScannedSeries>,
    pub sidecars: Vec<ScanSidecar>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesSidecarRule {
    pub filename: String,
    pub sidecar_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookSidecarRule {
    pub suffix: String,
    pub sidecar_type: String,
}

impl BookSidecarRule {
    fn prefilter_candidate(&self, filename: &str) -> bool {
        filename
            .to_ascii_lowercase()
            .ends_with(&self.suffix.to_ascii_lowercase())
    }

    fn matches_book(&self, book_name: &str, filename: &str) -> bool {
        let expected = format!("{}{}", book_name, self.suffix);
        expected.eq_ignore_ascii_case(filename)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScannerOptions {
    pub force_directory_modified_time: bool,
    pub oneshots_dir: Option<String>,
    pub scan_cbx: bool,
    pub scan_pdf: bool,
    pub scan_epub: bool,
    pub directory_exclusions: Vec<String>,
    pub series_sidecar_rules: Vec<SeriesSidecarRule>,
    pub book_sidecar_rules: Vec<BookSidecarRule>,
}

impl Default for ScannerOptions {
    fn default() -> Self {
        Self {
            force_directory_modified_time: false,
            oneshots_dir: None,
            scan_cbx: true,
            scan_pdf: true,
            scan_epub: true,
            directory_exclusions: Vec::new(),
            series_sidecar_rules: vec![SeriesSidecarRule {
                filename: "ComicInfo.xml".to_string(),
                sidecar_type: "COMIC_INFO_SERIES".to_string(),
            }],
            book_sidecar_rules: vec![BookSidecarRule {
                suffix: ".xml".to_string(),
                sidecar_type: "COMIC_INFO_BOOK".to_string(),
            }],
        }
    }
}

impl ScannerOptions {
    fn supported_extensions(&self) -> BTreeSet<&'static str> {
        let mut supported = BTreeSet::new();
        if self.scan_cbx {
            supported.insert("cbz");
            supported.insert("zip");
            supported.insert("cbr");
            supported.insert("rar");
        }
        if self.scan_pdf {
            supported.insert("pdf");
        }
        if self.scan_epub {
            supported.insert("epub");
        }
        supported
    }

    fn is_hidden_name(path: &Path) -> bool {
        path.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.'))
    }

    fn excluded_directory(&self, dir: &Path) -> bool {
        let haystack = dir.to_string_lossy().to_ascii_lowercase();
        self.directory_exclusions
            .iter()
            .any(|needle| haystack.contains(&needle.to_ascii_lowercase()))
    }

    fn is_oneshot_path(&self, path: &Path) -> bool {
        let Some(oneshots_dir) = self.oneshots_dir.as_ref() else {
            return false;
        };
        if oneshots_dir.trim().is_empty() {
            return false;
        }
        path.to_string_lossy()
            .to_ascii_lowercase()
            .contains(&oneshots_dir.to_ascii_lowercase())
    }

    fn is_supported_book_file(&self, path: &Path) -> bool {
        if Self::is_hidden_name(path) {
            return false;
        }
        let Some(extension) = path.extension().and_then(|it| it.to_str()) else {
            return false;
        };
        self.supported_extensions()
            .contains(extension.to_ascii_lowercase().as_str())
    }

    fn series_sidecar_for(&self, file_name: &str) -> Option<String> {
        self.series_sidecar_rules
            .iter()
            .find(|rule| rule.filename.eq_ignore_ascii_case(file_name))
            .map(|rule| rule.sidecar_type.clone())
    }

    fn book_sidecar_candidate(&self, file_name: &str) -> bool {
        self.book_sidecar_rules
            .iter()
            .any(|rule| rule.prefilter_candidate(file_name))
    }

    fn match_book_sidecar(&self, book_name: &str, file_name: &str) -> Option<String> {
        self.book_sidecar_rules
            .iter()
            .find(|rule| rule.matches_book(book_name, file_name))
            .map(|rule| rule.sidecar_type.clone())
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct ScanError {
    pub message: String,
}

impl Display for ScanError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ScanError {}

#[derive(Clone, Debug)]
struct TempSidecar {
    name: String,
    path: PathBuf,
    file_last_modified: SystemTime,
}

#[derive(Clone, Debug)]
struct TempSeries {
    name: String,
    path: PathBuf,
    file_last_modified: SystemTime,
}

#[derive(Clone, Debug, Default)]
struct TempScan {
    path_to_series: BTreeMap<PathBuf, TempSeries>,
    path_to_series_sidecars: BTreeMap<PathBuf, Vec<ScanSidecar>>,
    path_to_books: BTreeMap<PathBuf, Vec<ScanBook>>,
    path_to_book_sidecars: BTreeMap<PathBuf, Vec<TempSidecar>>,
    scanned_series: Vec<ScannedSeries>,
    scanned_sidecars: Vec<ScanSidecar>,
    visited_dirs: BTreeSet<PathBuf>,
}

pub fn scan_root_folder(root: &Path, options: &ScannerOptions) -> Result<ScanResult, ScanError> {
    validate_root(root)?;

    let mut temp = TempScan::default();
    recurse_directory(root, options, &mut temp);

    for sidecars in temp.path_to_series_sidecars.values() {
        temp.scanned_sidecars.extend(sidecars.iter().cloned());
    }

    temp.scanned_series
        .sort_by(|left, right| left.series.path.cmp(&right.series.path));
    temp.scanned_sidecars
        .sort_by(|left, right| left.path.cmp(&right.path));

    Ok(ScanResult {
        series: temp.scanned_series,
        sidecars: temp.scanned_sidecars,
    })
}

pub fn scan_file(path: &Path) -> Option<ScanBook> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() {
        return None;
    }

    Some(path_to_book(path, &metadata, false))
}

pub fn scan_book_sidecars(path: &Path, options: &ScannerOptions) -> Vec<ScanSidecar> {
    let Some(parent) = path.parent() else {
        return Vec::new();
    };
    let Some(book_name) = path.file_stem().and_then(|name| name.to_str()) else {
        return Vec::new();
    };

    let mut sidecars = Vec::new();
    let Ok(entries) = fs::read_dir(parent) else {
        return sidecars;
    };

    for entry in entries.flatten() {
        let candidate = entry.path();
        let Some(file_name) = candidate.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !options.book_sidecar_candidate(file_name) {
            continue;
        }
        let Some(sidecar_type) = options.match_book_sidecar(book_name, file_name) else {
            continue;
        };
        let Ok(metadata) = fs::metadata(&candidate) else {
            continue;
        };
        sidecars.push(ScanSidecar {
            path: candidate,
            target_path: parent.to_path_buf(),
            file_last_modified: updated_time(&metadata),
            sidecar_type,
            source: ScanSidecarSource::Book,
        });
    }

    sidecars.sort_by(|left, right| left.path.cmp(&right.path));
    sidecars
}

fn validate_root(root: &Path) -> Result<(), ScanError> {
    let metadata = fs::metadata(root).map_err(|_| ScanError {
        message: format!("Folder is not accessible: {}", root.display()),
    })?;
    if !metadata.is_dir() {
        return Err(ScanError {
            message: format!("Folder is not accessible: {}", root.display()),
        });
    }
    fs::read_dir(root).map_err(|_| ScanError {
        message: format!("Folder is not accessible: {}", root.display()),
    })?;
    Ok(())
}

fn recurse_directory(dir: &Path, options: &ScannerOptions, temp: &mut TempScan) {
    if options.excluded_directory(dir) || ScannerOptions::is_hidden_name(dir) {
        return;
    }

    let canonical = fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
    if !temp.visited_dirs.insert(canonical) {
        return;
    }

    let Ok(metadata) = fs::metadata(dir) else {
        return;
    };
    temp.path_to_series.insert(
        dir.to_path_buf(),
        TempSeries {
            name: directory_name_or_path(dir),
            path: dir.to_path_buf(),
            file_last_modified: updated_time(&metadata),
        },
    );

    let Ok(entries_iter) = fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries_iter.flatten().map(|entry| entry.path()).collect();
    entries.sort();

    for entry_path in entries {
        let Ok(symlink_metadata) = fs::symlink_metadata(&entry_path) else {
            continue;
        };

        let is_symlink = symlink_metadata.file_type().is_symlink();
        if is_symlink {
            let Ok(target_metadata) = fs::metadata(&entry_path) else {
                continue;
            };
            if target_metadata.is_dir() {
                recurse_directory(&entry_path, options, temp);
            }
            continue;
        }

        if symlink_metadata.is_dir() {
            recurse_directory(&entry_path, options, temp);
            continue;
        }

        if !symlink_metadata.is_file() {
            continue;
        }

        if options.is_supported_book_file(&entry_path) {
            let book = path_to_book(&entry_path, &symlink_metadata, false);
            if let Some(parent) = entry_path.parent() {
                temp.path_to_books
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push(book);
            }
        }

        let Some(file_name) = entry_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if let Some(sidecar_type) = options.series_sidecar_for(file_name)
            && let Some(parent) = entry_path.parent()
        {
            temp.path_to_series_sidecars
                .entry(parent.to_path_buf())
                .or_default()
                .push(ScanSidecar {
                    path: entry_path.clone(),
                    target_path: parent.to_path_buf(),
                    file_last_modified: updated_time(&symlink_metadata),
                    sidecar_type,
                    source: ScanSidecarSource::Series,
                });
        }

        if options.book_sidecar_candidate(file_name)
            && let Some(parent) = entry_path.parent()
        {
            temp.path_to_book_sidecars
                .entry(parent.to_path_buf())
                .or_default()
                .push(TempSidecar {
                    name: file_name.to_string(),
                    path: entry_path,
                    file_last_modified: updated_time(&symlink_metadata),
                });
        }
    }

    finalize_directory(dir, options, temp);
}

fn finalize_directory(dir: &Path, options: &ScannerOptions, temp: &mut TempScan) {
    let Some(books) = temp.path_to_books.get(dir).cloned() else {
        return;
    };
    if books.is_empty() {
        return;
    }
    let Some(temp_series) = temp.path_to_series.get(dir).cloned() else {
        return;
    };

    if options.is_oneshot_path(dir) {
        for book in &books {
            let series = ScanSeries {
                name: book.name.clone(),
                path: book.path.clone(),
                file_last_modified: book.file_last_modified,
                oneshot: true,
            };
            let mut oneshot_book = book.clone();
            oneshot_book.oneshot = true;
            temp.scanned_series.push(ScannedSeries {
                series,
                books: vec![oneshot_book],
            });
        }
    } else {
        let max_book_modified = books
            .iter()
            .map(|book| book.file_last_modified)
            .max()
            .unwrap_or(temp_series.file_last_modified);
        let series = ScanSeries {
            name: temp_series.name,
            path: temp_series.path,
            file_last_modified: if options.force_directory_modified_time {
                std::cmp::max(temp_series.file_last_modified, max_book_modified)
            } else {
                temp_series.file_last_modified
            },
            oneshot: false,
        };
        temp.scanned_series.push(ScannedSeries {
            series,
            books: books.clone(),
        });
    }

    let mut remaining_sidecars = temp.path_to_book_sidecars.remove(dir).unwrap_or_default();
    for book in &books {
        let mut matched_indexes = Vec::new();
        for (index, sidecar) in remaining_sidecars.iter().enumerate() {
            let Some(sidecar_type) = options.match_book_sidecar(&book.name, &sidecar.name) else {
                continue;
            };
            temp.scanned_sidecars.push(ScanSidecar {
                path: sidecar.path.clone(),
                target_path: book.path.clone(),
                file_last_modified: sidecar.file_last_modified,
                sidecar_type,
                source: ScanSidecarSource::Book,
            });
            matched_indexes.push(index);
        }

        for index in matched_indexes.into_iter().rev() {
            remaining_sidecars.remove(index);
        }
    }
}

fn directory_name_or_path(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn path_to_book(path: &Path, metadata: &fs::Metadata, oneshot: bool) -> ScanBook {
    ScanBook {
        name: path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        path: path.to_path_buf(),
        file_last_modified: updated_time(metadata),
        file_size: metadata.len(),
        oneshot,
    }
}

fn updated_time(metadata: &fs::Metadata) -> SystemTime {
    let created = metadata.created().unwrap_or(UNIX_EPOCH);
    let modified = metadata.modified().unwrap_or(UNIX_EPOCH);
    std::cmp::max(created, modified)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WatchTriggerKind {
    Scan,
    Import,
    Analyze,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct WatchTrigger {
    pub kind: WatchTriggerKind,
    pub path: PathBuf,
    pub oneshot: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WatchBackend {
    Notify,
    DeterministicFallback,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NotifyWatcherOptions {
    pub scanner: ScannerOptions,
}

pub struct NotifyLibraryWatcher {
    backend: WatchBackend,
    _watcher: Option<RecommendedWatcher>,
    options: NotifyWatcherOptions,
}

impl NotifyLibraryWatcher {
    pub fn new(root: &Path, options: NotifyWatcherOptions) -> Self {
        if !root.exists() {
            return Self {
                backend: WatchBackend::DeterministicFallback,
                _watcher: None,
                options,
            };
        }

        let mut watcher =
            match notify::recommended_watcher(|_event: Result<notify::Event, notify::Error>| {}) {
                Ok(watcher) => watcher,
                Err(_) => {
                    return Self {
                        backend: WatchBackend::DeterministicFallback,
                        _watcher: None,
                        options,
                    };
                }
            };

        match watcher.watch(root, RecursiveMode::Recursive) {
            Ok(()) => Self {
                backend: WatchBackend::Notify,
                _watcher: Some(watcher),
                options,
            },
            Err(_) => Self {
                backend: WatchBackend::DeterministicFallback,
                _watcher: None,
                options,
            },
        }
    }

    pub fn backend(&self) -> WatchBackend {
        self.backend
    }

    pub fn evaluate_changed_path(&self, path: &Path) -> Vec<WatchTrigger> {
        let oneshot = self.options.scanner.is_oneshot_path(path);

        if ScannerOptions::is_hidden_name(path) {
            return Vec::new();
        }

        if path.is_dir() {
            return vec![WatchTrigger {
                kind: WatchTriggerKind::Scan,
                path: path.to_path_buf(),
                oneshot,
            }];
        }

        let mut triggers = BTreeSet::new();
        if self.options.scanner.is_supported_book_file(path) {
            triggers.insert(WatchTrigger {
                kind: WatchTriggerKind::Import,
                path: path.to_path_buf(),
                oneshot,
            });
            triggers.insert(WatchTrigger {
                kind: WatchTriggerKind::Analyze,
                path: path.to_path_buf(),
                oneshot,
            });
        }

        if let Some(file_name) = path.file_name().and_then(|name| name.to_str()) {
            if self.options.scanner.series_sidecar_for(file_name).is_some() {
                triggers.insert(WatchTrigger {
                    kind: WatchTriggerKind::Scan,
                    path: path.to_path_buf(),
                    oneshot,
                });
            }

            if self.options.scanner.book_sidecar_candidate(file_name) {
                triggers.insert(WatchTrigger {
                    kind: WatchTriggerKind::Analyze,
                    path: path.to_path_buf(),
                    oneshot,
                });
            }
        }

        triggers.into_iter().collect()
    }

    pub fn evaluate_changed_paths<I>(&self, paths: I) -> Vec<WatchTrigger>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        let mut deduped = BTreeSet::new();
        for path in paths {
            for trigger in self.evaluate_changed_path(&path) {
                deduped.insert(trigger);
            }
        }
        deduped.into_iter().collect()
    }
}
