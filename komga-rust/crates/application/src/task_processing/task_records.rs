use super::{BookPayload, SeriesPayload, TaskKind, TaskQueueRecord, TaskRequest};

pub fn book_analyze_task_record(book_id: &str, series_id: &str) -> TaskQueueRecord {
    TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
        .priority(6)
        .group(series_id)
        .into_queue_record()
}

pub fn book_metadata_refresh_task_records(book_id: &str, series_id: &str) -> Vec<TaskQueueRecord> {
    vec![
        TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new(book_id))
            .priority(6)
            .group(series_id)
            .into_queue_record(),
        TaskRequest::with_payload(TaskKind::RefreshBookLocalArtwork, BookPayload::new(book_id))
            .priority(6)
            .into_queue_record(),
    ]
}

pub fn series_analyze_task_records(book_ids: Vec<String>, series_id: &str) -> Vec<TaskQueueRecord> {
    book_ids
        .into_iter()
        .map(|book_id| {
            TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
                .priority(6)
                .group(series_id.to_string())
                .into_queue_record()
        })
        .collect()
}

pub fn series_metadata_refresh_task_records(
    book_ids: Vec<String>,
    series_id: &str,
) -> Vec<TaskQueueRecord> {
    let mut records = Vec::with_capacity(book_ids.len() * 2 + 1);

    for book_id in book_ids {
        records.push(
            TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new(&book_id))
                .priority(6)
                .group(series_id.to_string())
                .into_queue_record(),
        );
        records.push(
            TaskRequest::with_payload(
                TaskKind::RefreshBookLocalArtwork,
                BookPayload::new(&book_id),
            )
            .priority(6)
            .into_queue_record(),
        );
    }

    records.push(
        TaskRequest::with_payload(
            TaskKind::RefreshSeriesLocalArtwork,
            SeriesPayload::new(series_id),
        )
        .priority(6)
        .into_queue_record(),
    );

    records
}
