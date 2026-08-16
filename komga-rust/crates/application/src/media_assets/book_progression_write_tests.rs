use std::sync::Mutex;

use serde_json::json;

use super::{
    BookAccessRestrictions, BookMediaRecord, BookProgressionConflictPolicy, BookProgressionInput,
    BookProgressionReaderPort, BookProgressionRecord, BookProgressionWrite,
    BookProgressionWriteError, BookProgressionWriteService, BookProgressionWriteSource,
    BookProgressionWriterPort,
};

#[tokio::test]
async fn book_progression_writer_rejects_stale_update_before_persisting() {
    let reader = TestProgressionReader {
        book_progression: Some(BookProgressionRecord {
            modified: "2026-06-07T12:00:00Z".to_string(),
            device_id: "device-1".to_string(),
            device_name: "Readium".to_string(),
            locator: json!({}),
        }),
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
                total_progression: Some(0.5),
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
    book_progression: Option<BookProgressionRecord>,
}

#[async_trait::async_trait]
impl BookProgressionReaderPort for TestProgressionReader {
    async fn book_media(&self, _book_id: &str) -> anyhow::Result<Option<BookMediaRecord>> {
        Ok(None)
    }

    async fn book_restrictions(
        &self,
        _book_id: &str,
    ) -> anyhow::Result<Option<BookAccessRestrictions>> {
        Ok(None)
    }

    async fn book_progression(
        &self,
        _book_id: &str,
        _user_id: &str,
    ) -> anyhow::Result<Option<BookProgressionRecord>> {
        Ok(self.book_progression.clone())
    }
}

#[derive(Default)]
struct TestProgressWriter {
    persisted: Mutex<Vec<BookProgressionInput>>,
}

#[async_trait::async_trait]
impl BookProgressionWriterPort for TestProgressWriter {
    async fn persist_book_progression(&self, input: BookProgressionInput) -> anyhow::Result<()> {
        self.persisted.lock().unwrap().push(input);
        Ok(())
    }
}
