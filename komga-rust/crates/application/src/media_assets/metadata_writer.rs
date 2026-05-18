use async_trait::async_trait;

use crate::task_processing::TaskQueueRecord;

use super::metadata_update::{BookMetadataPatch, BookMetadataPort, BookMetadataService};

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
    ) -> Result<MetadataUpdateResult, String> {
        let Some(series_id) = self.service.update_book_metadata(book_id, patch).await? else {
            return Ok(MetadataUpdateResult::NotFound);
        };

        self.search_sync.sync_book(book_id).await?;

        if let Some(series_id) = &series_id {
            self.enqueue_aggregate_series(series_id).await?;
        }

        self.emit_book_changed_from_port(book_id).await;

        Ok(MetadataUpdateResult::Updated)
    }

    pub async fn update_books_batch(
        &self,
        updates: Vec<(String, BookMetadataPatch)>,
    ) -> Result<MetadataUpdateResult, String> {
        let book_ids: Vec<String> = updates.iter().map(|(id, _)| id.clone()).collect();

        let affected_series_ids = self.service.batch_update_book_metadata(updates).await?;

        if !affected_series_ids.is_empty() {
            let records = affected_series_ids
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
            self.task_enqueuer.enqueue(records).await?;
        }

        for book_id in &book_ids {
            self.emit_book_changed_from_port(book_id).await;
        }

        Ok(MetadataUpdateResult::Updated)
    }

    async fn enqueue_aggregate_series(&self, series_id: &str) -> Result<(), String> {
        let record = crate::task_processing::TaskRequest::with_payload(
            crate::task_processing::TaskKind::AggregateSeriesMetadata,
            crate::task_processing::SeriesPayload::new(series_id),
        )
        .priority(80)
        .into_queue_record();
        self.task_enqueuer.enqueue(vec![record]).await
    }

    async fn emit_book_changed_from_port(&self, book_id: &str) {
        let series_id = self
            .service
            .port()
            .load_book_series_id(book_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        let library_id = self
            .service
            .port()
            .load_book_library_id(book_id)
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
        self.event_emitter
            .emit_book_changed(book_id, &series_id, &library_id);
    }
}
