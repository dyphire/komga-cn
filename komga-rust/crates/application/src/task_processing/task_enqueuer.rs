use super::protocol::LibraryTaskBatch;
use super::task_registry::{TaskKind, TaskRequest};

#[async_trait::async_trait]
pub trait TaskEnqueuer: Send + Sync {
    async fn enqueue(&self, kind: TaskKind, target_id: &str);

    async fn enqueue_request(&self, request: TaskRequest);

    async fn enqueue_batch(&self, batch: LibraryTaskBatch);
}

#[cfg(test)]
#[allow(dead_code)]
pub struct InMemoryTaskEnqueuer {
    pub submitted: std::sync::Mutex<Vec<TaskRequest>>,
}

#[cfg(test)]
#[allow(dead_code)]
impl InMemoryTaskEnqueuer {
    pub fn new() -> Self {
        Self {
            submitted: std::sync::Mutex::new(Vec::new()),
        }
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl TaskEnqueuer for InMemoryTaskEnqueuer {
    async fn enqueue(&self, kind: TaskKind, _target_id: &str) {
        let request = TaskRequest::new(kind);
        self.submitted.lock().unwrap().push(request);
    }

    async fn enqueue_request(&self, request: TaskRequest) {
        self.submitted.lock().unwrap().push(request);
    }

    async fn enqueue_batch(&self, batch: LibraryTaskBatch) {
        for record in batch.into_queue_records() {
            let request = TaskRequest::new(TaskKind::parse(&record.simple_type).unwrap());
            self.submitted.lock().unwrap().push(request);
        }
    }
}
