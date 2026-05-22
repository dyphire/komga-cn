use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tsid::create_tsid_256;

use super::{
    TransientBookAnalysis, TransientBookFileMetadata, TransientBookPage, TransientBookPort,
    TransientBookScanEntry,
};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct TransientBooksStore {
    pub records: HashMap<String, TransientBookRecord>,
    #[serde(default)]
    last_access_epoch_seconds: HashMap<String, i64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TransientBookRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
    pub status: String,
    pub media_type: String,
    #[serde(default)]
    pub page_count: u32,
    #[serde(default)]
    pub pages: Vec<TransientBookPage>,
    #[serde(default)]
    pub files: Vec<String>,
    #[serde(default)]
    pub comment: String,
    #[serde(default)]
    pub number: Option<f64>,
    #[serde(default)]
    pub series_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransientBookScanError {
    BadRequest(String),
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransientBookPageError {
    NotFound,
    AnalysisFailed,
    FileMissing,
    BadPageNumber,
    Internal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransientBookPageContent {
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone)]
pub struct TransientBookService {
    port: Arc<dyn TransientBookPort>,
    store: Arc<Mutex<TransientBooksStore>>,
}

impl TransientBookService {
    pub fn new(port: Arc<dyn TransientBookPort>) -> Self {
        Self::with_store(port, TransientBooksStore::default())
    }

    pub fn with_store(port: Arc<dyn TransientBookPort>, store: TransientBooksStore) -> Self {
        Self {
            port,
            store: Arc::new(Mutex::new(store)),
        }
    }

    pub async fn scan(
        &self,
        requested_path: &str,
    ) -> Result<Vec<TransientBookRecord>, TransientBookScanError> {
        match self.port.validate_transient_scan_root(requested_path).await {
            Ok(()) => {}
            Err(error_code) if matches!(error_code.as_str(), "ERR_1016" | "ERR_1017") => {
                return Err(TransientBookScanError::BadRequest(error_code));
            }
            Err(_) => return Err(TransientBookScanError::Internal),
        }

        let mut records = self
            .port
            .list_transient_book_entries(Path::new(requested_path))
            .into_iter()
            .filter_map(|entry| self.transient_book_record(entry))
            .collect::<Vec<_>>();
        records.sort_by(|left, right| left.path.cmp(&right.path));

        let mut store = self.lock_store();
        for record in &records {
            store.insert(record.clone());
        }

        Ok(records)
    }

    pub async fn analyze(&self, transient_book_id: &str) -> Option<TransientBookRecord> {
        let record = self.lock_store().get_cloned(transient_book_id)?;
        let analysis = self.port.analyze_transient_book(&record.path);
        let (inferred_series_id, inferred_number) = self
            .port
            .infer_transient_series_and_number(&record.path)
            .await;

        let mut store = self.lock_store();
        let entry = store.get_mut(transient_book_id)?;
        apply_transient_book_analysis(entry, analysis, inferred_series_id, inferred_number);
        Some(entry.clone())
    }

    pub fn page_content(
        &self,
        transient_book_id: &str,
        page_number: i32,
    ) -> Result<TransientBookPageContent, TransientBookPageError> {
        if page_number <= 0 {
            return Err(TransientBookPageError::BadPageNumber);
        }
        let page_number = page_number as u32;

        let record = self
            .lock_store()
            .get_cloned(transient_book_id)
            .ok_or(TransientBookPageError::NotFound)?;
        if !record.status.eq_ignore_ascii_case("READY") {
            return Err(TransientBookPageError::AnalysisFailed);
        }
        if !self.port.transient_book_exists(&record.path) {
            return Err(TransientBookPageError::FileMissing);
        }
        if record.media_type == "application/epub+zip" && record.pages.is_empty() {
            if record.page_count > 0 && page_number > record.page_count {
                return Err(TransientBookPageError::BadPageNumber);
            }
            return Err(TransientBookPageError::Internal);
        }

        let (content_type, bytes) = self
            .port
            .transient_book_page_content(
                &record.path,
                &record.media_type,
                &record.pages,
                page_number,
            )
            .ok_or(TransientBookPageError::BadPageNumber)?;

        Ok(TransientBookPageContent {
            content_type,
            bytes,
        })
    }

    fn transient_book_record(&self, entry: TransientBookScanEntry) -> Option<TransientBookRecord> {
        let file_metadata = self.port.load_transient_book_file_metadata(&entry.path)?;

        Some(unknown_transient_book_record(entry, file_metadata))
    }

    fn lock_store(&self) -> MutexGuard<'_, TransientBooksStore> {
        self.store
            .lock()
            .expect("transient books state lock should not be poisoned")
    }
}

impl TransientBooksStore {
    pub fn with_records(records: HashMap<String, TransientBookRecord>) -> Self {
        let last_access_epoch_seconds = records
            .keys()
            .cloned()
            .map(|id| (id, current_unix_epoch_seconds()))
            .collect();
        Self {
            records,
            last_access_epoch_seconds,
        }
    }

    pub fn get_cloned(&mut self, id: &str) -> Option<TransientBookRecord> {
        self.prune_expired();
        self.touch(id)?;
        self.records.get(id).cloned()
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut TransientBookRecord> {
        self.prune_expired();
        self.touch(id)?;
        self.records.get_mut(id)
    }

    pub fn insert(&mut self, record: TransientBookRecord) {
        self.prune_expired();
        let id = record.id.clone();
        self.last_access_epoch_seconds
            .insert(id.clone(), current_unix_epoch_seconds());
        self.records.insert(id, record);
    }

    fn prune_expired(&mut self) {
        let now = current_unix_epoch_seconds();
        let expired_ids = self
            .last_access_epoch_seconds
            .iter()
            .filter(|(_, last_access)| {
                now.saturating_sub(**last_access)
                    >= TRANSIENT_BOOKS_EXPIRE_AFTER_ACCESS.as_secs() as i64
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();

        for id in expired_ids {
            self.last_access_epoch_seconds.remove(&id);
            self.records.remove(&id);
        }
    }

    fn touch(&mut self, id: &str) -> Option<()> {
        if !self.records.contains_key(id) {
            self.last_access_epoch_seconds.remove(id);
            return None;
        }

        self.last_access_epoch_seconds
            .insert(id.to_string(), current_unix_epoch_seconds());
        Some(())
    }
}

fn unknown_transient_book_record(
    entry: TransientBookScanEntry,
    file_metadata: TransientBookFileMetadata,
) -> TransientBookRecord {
    TransientBookRecord {
        id: transient_book_id(),
        name: entry.name,
        path: entry.path,
        file_last_modified_unix_nanos: file_metadata.file_last_modified_unix_nanos,
        size_bytes: file_metadata.size_bytes,
        status: "UNKNOWN".to_string(),
        media_type: String::new(),
        page_count: 0,
        pages: Vec::new(),
        files: Vec::new(),
        comment: String::new(),
        number: None,
        series_id: None,
    }
}

fn apply_transient_book_analysis(
    entry: &mut TransientBookRecord,
    analysis: TransientBookAnalysis,
    inferred_series_id: Option<String>,
    inferred_number: Option<f64>,
) {
    entry.status = analysis.status;
    entry.media_type = analysis.media_type;
    entry.page_count = analysis.page_count;
    entry.pages = analysis.pages;
    entry.files = analysis.files;
    entry.comment = analysis.comment;
    entry.number = analysis.number.or(inferred_number);
    entry.series_id = analysis.series_id.or(inferred_series_id);
}

fn transient_book_id() -> String {
    create_tsid_256().to_string()
}

const TRANSIENT_BOOKS_EXPIRE_AFTER_ACCESS: Duration = Duration::from_secs(60 * 60);

fn current_unix_epoch_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::transient_book_id;

    #[test]
    fn transient_book_id_uses_kotlin_compatible_tsid_shape() {
        let id = transient_book_id();

        assert_eq!(id.len(), 13);
        assert!(matches!(id.chars().next(), Some('0'..='9' | 'A'..='F')));
        assert!(
            id.chars()
                .all(|ch| "0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(ch))
        );
    }
}
