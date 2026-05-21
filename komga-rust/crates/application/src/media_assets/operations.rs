use crate::discovery::BookDetailPort;
use crate::media_assets::SeriesRelationPort;
use crate::task_processing::{BookPayload, SeriesPayload, TaskKind, TaskQueueRecord, TaskRequest};

pub async fn derive_book_analyze_tasks(
    book_detail: &dyn BookDetailPort,
    book_id: &str,
) -> Result<Option<Vec<TaskQueueRecord>>, String> {
    let Some(book) = book_detail
        .load_persisted_book_detail(book_id, None)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(vec![
        TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
            .priority(6)
            .group(book.series_id)
            .into_queue_record(),
    ]))
}

pub async fn derive_book_metadata_refresh_tasks(
    book_detail: &dyn BookDetailPort,
    book_id: &str,
) -> Result<Option<Vec<TaskQueueRecord>>, String> {
    let Some(book) = book_detail
        .load_persisted_book_detail(book_id, None)
        .await?
    else {
        return Ok(None);
    };
    Ok(Some(vec![
        TaskRequest::with_payload(TaskKind::RefreshBookMetadata, BookPayload::new(book_id))
            .priority(6)
            .group(book.series_id.clone())
            .into_queue_record(),
        TaskRequest::with_payload(TaskKind::RefreshBookLocalArtwork, BookPayload::new(book_id))
            .priority(6)
            .into_queue_record(),
    ]))
}

pub async fn derive_series_analyze_tasks(
    series_relation: &dyn SeriesRelationPort,
    series_id: &str,
) -> Result<Vec<TaskQueueRecord>, String> {
    let book_ids = series_relation.series_book_ids(series_id).await?;
    let records = book_ids
        .into_iter()
        .map(|book_id| {
            TaskRequest::with_payload(TaskKind::AnalyzeBook, BookPayload::new(book_id))
                .priority(6)
                .group(series_id.to_string())
                .into_queue_record()
        })
        .collect();
    Ok(records)
}

pub async fn derive_series_metadata_refresh_tasks(
    series_relation: &dyn SeriesRelationPort,
    series_id: &str,
) -> Result<Vec<TaskQueueRecord>, String> {
    let book_ids = series_relation.series_book_ids(series_id).await?;
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
    Ok(records)
}
