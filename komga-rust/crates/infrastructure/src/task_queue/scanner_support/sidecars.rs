use super::*;

pub(in crate::task_queue) fn enqueue_sidecar_refresh_tasks(
    tasks: &mut Vec<TaskQueueRecord>,
    scanned: &ScannedLibrary,
    changed_sidecar_urls: &[String],
    priority: i32,
) {
    let changed_sidecar_urls = changed_sidecar_urls.iter().cloned().collect::<HashSet<_>>();
    let mut series_by_url = HashMap::new();
    let mut book_by_url = HashMap::new();
    for series in &scanned.series_rows {
        series_by_url.insert(series.series_url.clone(), series.series_id.clone());
        for book in &series.books {
            book_by_url.insert(book.book_url.clone(), book.book_id.clone());
        }
    }

    let mut seen_series_metadata: HashSet<String> = HashSet::new();
    let mut seen_series_artwork: HashSet<String> = HashSet::new();
    let mut seen_books_metadata: HashSet<String> = HashSet::new();
    let mut seen_books_artwork: HashSet<String> = HashSet::new();
    let mut book_series_by_url = HashMap::new();
    for series in &scanned.series_rows {
        for book in &series.books {
            book_series_by_url.insert(book.book_url.clone(), series.series_id.clone());
        }
    }
    for sidecar in &scanned.sidecars {
        if !changed_sidecar_urls.contains(&sidecar.url) {
            continue;
        }
        if sidecar.last_modified_unix_seconds < 0 {
            continue;
        }

        match (sidecar.source, sidecar.sidecar_type) {
            (ScannedSidecarSource::Series, ScannedSidecarType::Metadata) => {
                if let Some(series_id) = series_by_url.get(&sidecar.parent_url)
                    && seen_series_metadata.insert(series_id.clone())
                {
                    tasks.push(runtime_follow_up_task(
                        RuntimeFollowUpTask::RefreshSeriesMetadata {
                            series_id: series_id.clone(),
                            priority,
                        },
                    ));
                }
            }
            (ScannedSidecarSource::Series, ScannedSidecarType::Artwork) => {
                if let Some(series_id) = series_by_url.get(&sidecar.parent_url)
                    && seen_series_artwork.insert(series_id.clone())
                {
                    tasks.push(runtime_follow_up_task(
                        RuntimeFollowUpTask::RefreshSeriesLocalArtwork {
                            series_id: series_id.clone(),
                            priority,
                        },
                    ));
                }
            }
            (ScannedSidecarSource::Book, ScannedSidecarType::Metadata) => {
                if let Some(book_id) = book_by_url.get(&sidecar.parent_url)
                    && seen_books_metadata.insert(book_id.clone())
                {
                    let group_id = book_series_by_url.get(&sidecar.parent_url).cloned();
                    tasks.push(runtime_follow_up_task(
                        RuntimeFollowUpTask::RefreshBookMetadata {
                            book_id: book_id.clone(),
                            series_id: group_id,
                            priority,
                            capabilities: None,
                        },
                    ));
                }
            }
            (ScannedSidecarSource::Book, ScannedSidecarType::Artwork) => {
                if let Some(book_id) = book_by_url.get(&sidecar.parent_url)
                    && seen_books_artwork.insert(book_id.clone())
                {
                    tasks.push(runtime_follow_up_task(
                        RuntimeFollowUpTask::RefreshBookLocalArtwork {
                            book_id: book_id.clone(),
                            priority,
                        },
                    ));
                }
            }
        }
    }
}
