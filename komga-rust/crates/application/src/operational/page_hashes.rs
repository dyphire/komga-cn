use std::sync::Arc;

use crate::media_assets::PageHashDeleteTargetPage;
use crate::task_processing::{
    HashedPageToDeletePayload, RemoveHashedPagesPayload, SubmitUrgency, TaskKind, TaskQueueAdmin,
    TaskQueueRecord, TaskRequest,
};

use super::PageHashPort;

const REMOVE_HASHED_PAGES_PRIORITY: i32 = 4;

#[derive(Clone)]
pub struct PageHashService {
    page_hashes: Arc<dyn PageHashPort>,
    task_queue: Arc<dyn TaskQueueAdmin>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashDeleteMatch {
    pub book_id: String,
    pub page_hash: String,
    pub page_number: i64,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageHashDeleteError {
    LoadTargets(String),
    Enqueue(String),
}

impl PageHashService {
    pub fn new(page_hashes: Arc<dyn PageHashPort>, task_queue: Arc<dyn TaskQueueAdmin>) -> Self {
        Self {
            page_hashes,
            task_queue,
        }
    }

    pub async fn enqueue_delete_all(&self, page_hash: &str) -> Result<(), PageHashDeleteError> {
        let targets = self
            .page_hashes
            .load_page_hash_delete_targets(page_hash)
            .await
            .map_err(PageHashDeleteError::LoadTargets)?;
        let records = targets
            .into_iter()
            .map(|target| remove_hashed_pages_task(target.book_id, target.pages))
            .collect::<Vec<_>>();

        self.enqueue_remove_hashed_pages(records).await
    }

    pub async fn enqueue_delete_match(
        &self,
        delete_match: PageHashDeleteMatch,
    ) -> Result<(), PageHashDeleteError> {
        let pages = vec![HashedPageToDeletePayload {
            file_hash: delete_match.page_hash,
            file_size: delete_match.file_size,
            file_name: delete_match.file_name,
            media_type: delete_match.media_type,
            page_number: delete_match.page_number,
        }];
        let record = remove_hashed_pages_task(delete_match.book_id, pages);

        self.enqueue_remove_hashed_pages(vec![record]).await
    }

    async fn enqueue_remove_hashed_pages(
        &self,
        records: Vec<TaskQueueRecord>,
    ) -> Result<(), PageHashDeleteError> {
        self.task_queue
            .enqueue_records(records, SubmitUrgency::Immediate)
            .await
            .map_err(PageHashDeleteError::Enqueue)
    }
}

fn remove_hashed_pages_task(
    book_id: String,
    pages: Vec<impl Into<HashedPageToDeletePayload>>,
) -> TaskQueueRecord {
    let pages = pages.into_iter().map(Into::into).collect();
    TaskRequest::with_payload(
        TaskKind::RemoveHashedPages,
        RemoveHashedPagesPayload::new(book_id, pages),
    )
    .priority(REMOVE_HASHED_PAGES_PRIORITY)
    .into_queue_record()
}

impl From<PageHashDeleteTargetPage> for HashedPageToDeletePayload {
    fn from(page: PageHashDeleteTargetPage) -> Self {
        Self {
            file_hash: page.file_hash,
            file_size: page.file_size,
            file_name: page.file_name,
            media_type: page.media_type,
            page_number: page.page_number,
        }
    }
}
