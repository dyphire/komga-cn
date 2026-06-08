use std::sync::Mutex;

use async_trait::async_trait;
use serde_json::{Value, json};

use super::{
    BookMediaRecord, BookProgressionConflictPolicy, BookProgressionInput,
    BookProgressionReaderPort, BookProgressionWrite, BookProgressionWriteError,
    BookProgressionWriteService, BookProgressionWriteSource, ProgressWriterPort,
};

#[tokio::test]
async fn book_progression_writer_rejects_stale_update_before_persisting() {
    let reader = TestProgressionReader {
        book_progression: Some(json!({
            "modified": "2026-06-07T12:00:00Z"
        })),
    };
    let progress = TestProgressWriter::default();
    let writer = BookProgressionWriteService::new(&reader, &progress);

    let result = writer
        .persist(BookProgressionWrite {
            book_id: "book-1".to_string(),
            user_id: "user-1".to_string(),
            page_count: 10,
            source: BookProgressionWriteSource::TotalProgression {
                progression: 0.5,
                locator: Some(json!({
                    "locations": {
                        "totalProgression": 0.5
                    }
                })),
            },
            modified: Some("2026-06-07T12:00:00Z".to_string()),
            device_id: Some("device-1".to_string()),
            device_name: Some("Readium".to_string()),
            conflict_policy: BookProgressionConflictPolicy::RejectStale,
        })
        .await;

    assert_eq!(result, Err(BookProgressionWriteError::Stale));
    assert!(progress.persisted.lock().unwrap().is_empty());
}

#[derive(Default)]
struct TestProgressionReader {
    book_progression: Option<Value>,
}

#[async_trait]
impl BookProgressionReaderPort for TestProgressionReader {
    async fn book_media(&self, _book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        Ok(None)
    }

    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        Ok(None)
    }

    async fn book_progression(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> Result<Option<Value>, String> {
        Ok(self.book_progression.clone())
    }
}

#[derive(Default)]
struct TestProgressWriter {
    persisted: Mutex<Vec<BookProgressionInput>>,
}

#[async_trait]
impl ProgressWriterPort for TestProgressWriter {
    async fn persist_read_progress(
        &self,
        _book_id: &str,
        _user_id: &str,
        _page: u64,
        _completed: bool,
        _locator: Option<Value>,
    ) -> Result<(), String> {
        unreachable!("page progress writes are not part of this test")
    }

    async fn persist_book_progression(&self, input: BookProgressionInput) -> Result<(), String> {
        self.persisted.lock().unwrap().push(input);
        Ok(())
    }

    async fn delete_read_progress(&self, _book_id: &str, _user_id: &str) -> Result<(), String> {
        unreachable!("delete progress is not part of this test")
    }

    async fn persist_readlist_tachiyomi_progress(
        &self,
        _ordered_book_ids: &[String],
        _user_id: &str,
        _last_book_read: usize,
    ) -> Result<Option<()>, String> {
        unreachable!("readlist progress is not part of this test")
    }

    async fn refresh_series_read_progress(
        &self,
        _series_id: &str,
        _user_id: &str,
    ) -> Result<(), String> {
        unreachable!("series progress is not part of this test")
    }

    async fn delete_series_read_progress(
        &self,
        _series_id: &str,
        _user_id: &str,
    ) -> Result<(), String> {
        unreachable!("series progress is not part of this test")
    }
}
