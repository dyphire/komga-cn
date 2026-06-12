use async_trait::async_trait;

use crate::task_processing::TaskQueueRecord;

use super::metadata_update::{
    BookMetadataPatch, BookMetadataPort, BookMetadataService, BookMetadataUpdate,
    BookMetadataUpdateError,
};

// --- Side-effect ports (trait only where polymorphism is needed for testing) ---

#[async_trait]
pub trait SearchSyncPort: Send + Sync {
    async fn sync_book(&self, book_id: &str) -> Result<(), String>;
}

#[async_trait]
pub trait TaskEnqueuePort: Send + Sync {
    async fn enqueue(&self, records: Vec<TaskQueueRecord>) -> Result<(), String>;
}

pub trait BookEventEmitter: Send + Sync {
    fn emit_book_changed(&self, book_id: &str, series_id: &str, library_id: &str);
}

// --- MetadataWriter ---

pub enum MetadataUpdateResult {
    Updated,
    NotFound,
}

pub struct MetadataWriter {
    service: BookMetadataService,
    search_sync: Box<dyn SearchSyncPort>,
    task_enqueuer: Box<dyn TaskEnqueuePort>,
    event_emitter: Box<dyn BookEventEmitter>,
}

impl MetadataWriter {
    pub fn new(
        port: Box<dyn BookMetadataPort>,
        search_sync: Box<dyn SearchSyncPort>,
        task_enqueuer: Box<dyn TaskEnqueuePort>,
        event_emitter: Box<dyn BookEventEmitter>,
    ) -> Self {
        Self {
            service: BookMetadataService::new(port),
            search_sync,
            task_enqueuer,
            event_emitter,
        }
    }

    pub async fn update_book(
        &self,
        book_id: &str,
        patch: &BookMetadataPatch,
    ) -> Result<MetadataUpdateResult, BookMetadataUpdateError> {
        let Some(series_id) = self.service.update_book_metadata(book_id, patch).await? else {
            return Ok(MetadataUpdateResult::NotFound);
        };

        self.search_sync
            .sync_book(book_id)
            .await
            .map_err(BookMetadataUpdateError::persistence)?;

        if let Some(series_id) = &series_id {
            self.enqueue_aggregate_series(series_id).await?;
        }

        self.emit_book_changed_from_port(book_id).await?;

        Ok(MetadataUpdateResult::Updated)
    }

    pub async fn update_books_batch(
        &self,
        updates: Vec<BookMetadataUpdate>,
    ) -> Result<MetadataUpdateResult, BookMetadataUpdateError> {
        let outcome = self.service.batch_update_book_metadata(updates).await?;

        if !outcome.affected_series_ids.is_empty() {
            let records = outcome
                .affected_series_ids
                .iter()
                .map(|series_id| {
                    crate::task_processing::TaskRequest::with_payload(
                        crate::task_processing::TaskKind::AggregateSeriesMetadata,
                        crate::task_processing::SeriesPayload::new(series_id),
                    )
                    .priority(80)
                    .into_queue_record()
                })
                .collect();
            self.task_enqueuer
                .enqueue(records)
                .await
                .map_err(BookMetadataUpdateError::persistence)?;
        }

        for book_id in &outcome.updated_book_ids {
            self.emit_book_changed_from_port(book_id).await?;
        }

        Ok(MetadataUpdateResult::Updated)
    }

    async fn enqueue_aggregate_series(
        &self,
        series_id: &str,
    ) -> Result<(), BookMetadataUpdateError> {
        let record = crate::task_processing::TaskRequest::with_payload(
            crate::task_processing::TaskKind::AggregateSeriesMetadata,
            crate::task_processing::SeriesPayload::new(series_id),
        )
        .priority(80)
        .into_queue_record();
        self.task_enqueuer
            .enqueue(vec![record])
            .await
            .map_err(BookMetadataUpdateError::persistence)
    }

    async fn emit_book_changed_from_port(
        &self,
        book_id: &str,
    ) -> Result<(), BookMetadataUpdateError> {
        let series_id = self
            .service
            .port()
            .load_book_series_id(book_id)
            .await
            .map_err(BookMetadataUpdateError::persistence)?
            .ok_or_else(|| {
                BookMetadataUpdateError::persistence(format!(
                    "book event series context missing for '{book_id}'"
                ))
            })?;
        let library_id = self
            .service
            .port()
            .load_book_library_id(book_id)
            .await
            .map_err(BookMetadataUpdateError::persistence)?
            .ok_or_else(|| {
                BookMetadataUpdateError::persistence(format!(
                    "book event library context missing for '{book_id}'"
                ))
            })?;
        self.event_emitter
            .emit_book_changed(book_id, &series_id, &library_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use std::sync::{Arc, Mutex};

    use crate::task_processing::TaskQueueRecord;

    use super::super::metadata_update::{
        BookMetadata, BookMetadataPatch, BookMetadataPort, BookMetadataUpdate,
        BookMetadataUpdateError,
    };
    use super::{
        BookEventEmitter, MetadataUpdateResult, MetadataWriter, SearchSyncPort, TaskEnqueuePort,
    };

    #[tokio::test]
    async fn update_book_propagates_event_context_query_errors() {
        let writer = MetadataWriter::new(
            Box::new(EventContextFailingMetadataPort),
            Box::new(NoopSearchSyncPort),
            Box::new(NoopTaskEnqueuePort),
            Box::new(PanicBookEventEmitter),
        );

        let result = writer
            .update_book(
                "book-1",
                &BookMetadataPatch {
                    title: Some("Updated".to_string()),
                    ..BookMetadataPatch::default()
                },
            )
            .await;
        let Err(error) = result else {
            panic!("event context lookup errors must propagate");
        };

        assert_eq!(
            error,
            BookMetadataUpdateError::Persistence("library lookup failed".to_string()),
        );
    }

    #[tokio::test]
    async fn update_book_rejects_missing_event_context() {
        let writer = MetadataWriter::new(
            Box::new(EventContextMissingMetadataPort),
            Box::new(NoopSearchSyncPort),
            Box::new(NoopTaskEnqueuePort),
            Box::new(PanicBookEventEmitter),
        );

        let result = writer
            .update_book(
                "book-1",
                &BookMetadataPatch {
                    title: Some("Updated".to_string()),
                    ..BookMetadataPatch::default()
                },
            )
            .await;
        let Err(error) = result else {
            panic!("missing event context must not emit empty-id events");
        };

        assert_eq!(
            error,
            BookMetadataUpdateError::Persistence(
                "book event series context missing for 'book-1'".to_string(),
            ),
        );
    }

    #[tokio::test]
    async fn batch_update_emits_and_enqueues_only_persisted_books() {
        let enqueued_records = Arc::new(Mutex::new(Vec::new()));
        let emitted_events = Arc::new(Mutex::new(Vec::new()));
        let writer = MetadataWriter::new(
            Box::new(SelectiveBatchMetadataPort),
            Box::new(NoopSearchSyncPort),
            Box::new(RecordingTaskEnqueuePort {
                records: enqueued_records.clone(),
            }),
            Box::new(RecordingBookEventEmitter {
                events: emitted_events.clone(),
            }),
        );

        let result = writer
            .update_books_batch(vec![
                BookMetadataUpdate {
                    book_id: "book-1".to_string(),
                    patch: BookMetadataPatch {
                        title: Some("Updated".to_string()),
                        ..BookMetadataPatch::default()
                    },
                },
                BookMetadataUpdate {
                    book_id: "stale-book".to_string(),
                    patch: BookMetadataPatch {
                        title: Some("Stale".to_string()),
                        ..BookMetadataPatch::default()
                    },
                },
                BookMetadataUpdate {
                    book_id: "missing-book".to_string(),
                    patch: BookMetadataPatch {
                        title: Some("Missing".to_string()),
                        ..BookMetadataPatch::default()
                    },
                },
            ])
            .await
            .expect("batch metadata update should complete");

        assert!(matches!(result, MetadataUpdateResult::Updated));
        assert_eq!(
            enqueued_records
                .lock()
                .unwrap()
                .iter()
                .map(|record| record.target().unwrap_or_default().to_string())
                .collect::<Vec<_>>(),
            vec!["series-1".to_string()],
        );
        assert_eq!(
            *emitted_events.lock().unwrap(),
            vec![(
                "book-1".to_string(),
                "series-1".to_string(),
                "library-1".to_string(),
            )],
        );
    }

    struct EventContextFailingMetadataPort;

    #[async_trait]
    impl BookMetadataPort for EventContextFailingMetadataPort {
        async fn load_book_metadata(&self, _book_id: &str) -> Result<Option<BookMetadata>, String> {
            Ok(Some(sample_metadata()))
        }

        async fn load_book_series_id(&self, _book_id: &str) -> Result<Option<String>, String> {
            Ok(Some("series-1".to_string()))
        }

        async fn load_book_library_id(&self, _book_id: &str) -> Result<Option<String>, String> {
            Err("library lookup failed".to_string())
        }

        async fn persist_book_metadata(
            &self,
            _book_id: &str,
            _metadata: &BookMetadata,
        ) -> Result<bool, String> {
            Ok(true)
        }
    }

    struct EventContextMissingMetadataPort;

    #[async_trait]
    impl BookMetadataPort for EventContextMissingMetadataPort {
        async fn load_book_metadata(&self, _book_id: &str) -> Result<Option<BookMetadata>, String> {
            Ok(Some(sample_metadata()))
        }

        async fn load_book_series_id(&self, _book_id: &str) -> Result<Option<String>, String> {
            Ok(None)
        }

        async fn load_book_library_id(&self, _book_id: &str) -> Result<Option<String>, String> {
            Ok(Some("library-1".to_string()))
        }

        async fn persist_book_metadata(
            &self,
            _book_id: &str,
            _metadata: &BookMetadata,
        ) -> Result<bool, String> {
            Ok(true)
        }
    }

    struct SelectiveBatchMetadataPort;

    #[async_trait]
    impl BookMetadataPort for SelectiveBatchMetadataPort {
        async fn load_book_metadata(&self, book_id: &str) -> Result<Option<BookMetadata>, String> {
            match book_id {
                "book-1" | "stale-book" => Ok(Some(sample_metadata())),
                _ => Ok(None),
            }
        }

        async fn load_book_series_id(&self, book_id: &str) -> Result<Option<String>, String> {
            match book_id {
                "book-1" => Ok(Some("series-1".to_string())),
                "stale-book" => Ok(Some("series-stale".to_string())),
                _ => Ok(None),
            }
        }

        async fn load_book_library_id(&self, book_id: &str) -> Result<Option<String>, String> {
            match book_id {
                "book-1" => Ok(Some("library-1".to_string())),
                "stale-book" => Ok(Some("library-stale".to_string())),
                _ => Ok(None),
            }
        }

        async fn persist_book_metadata(
            &self,
            book_id: &str,
            _metadata: &BookMetadata,
        ) -> Result<bool, String> {
            Ok(book_id == "book-1")
        }
    }

    struct NoopSearchSyncPort;

    #[async_trait]
    impl SearchSyncPort for NoopSearchSyncPort {
        async fn sync_book(&self, _book_id: &str) -> Result<(), String> {
            Ok(())
        }
    }

    struct NoopTaskEnqueuePort;

    #[async_trait]
    impl TaskEnqueuePort for NoopTaskEnqueuePort {
        async fn enqueue(&self, _records: Vec<TaskQueueRecord>) -> Result<(), String> {
            Ok(())
        }
    }

    struct RecordingTaskEnqueuePort {
        records: Arc<Mutex<Vec<TaskQueueRecord>>>,
    }

    #[async_trait]
    impl TaskEnqueuePort for RecordingTaskEnqueuePort {
        async fn enqueue(&self, records: Vec<TaskQueueRecord>) -> Result<(), String> {
            self.records.lock().unwrap().extend(records);
            Ok(())
        }
    }

    struct PanicBookEventEmitter;

    impl BookEventEmitter for PanicBookEventEmitter {
        fn emit_book_changed(&self, _book_id: &str, _series_id: &str, _library_id: &str) {
            panic!("failed event context lookup must not emit empty-id events");
        }
    }

    struct RecordingBookEventEmitter {
        events: Arc<Mutex<Vec<(String, String, String)>>>,
    }

    impl BookEventEmitter for RecordingBookEventEmitter {
        fn emit_book_changed(&self, book_id: &str, series_id: &str, library_id: &str) {
            self.events.lock().unwrap().push((
                book_id.to_string(),
                series_id.to_string(),
                library_id.to_string(),
            ));
        }
    }

    fn sample_metadata() -> BookMetadata {
        BookMetadata {
            title: "Book 1".to_string(),
            title_lock: false,
            summary: String::new(),
            summary_lock: false,
            number: "1".to_string(),
            number_lock: false,
            number_sort: 1.0,
            number_sort_lock: false,
            release_date: None,
            release_date_lock: false,
            authors: Vec::new(),
            authors_lock: false,
            tags: Vec::new(),
            tags_lock: false,
            isbn: String::new(),
            isbn_lock: false,
            links: Vec::new(),
            links_lock: false,
        }
    }
}
