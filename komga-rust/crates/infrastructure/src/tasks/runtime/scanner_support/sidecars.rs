use super::*;

pub(in crate::task_queue) fn enqueue_sidecar_refresh_tasks(
    scheduler: &mut TaskQueueScheduler,
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
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_SERIES_METADATA:{series_id}"),
                        priority,
                        Some(series_id.clone()),
                    ));
                }
            }
            (ScannedSidecarSource::Series, ScannedSidecarType::Artwork) => {
                if let Some(series_id) = series_by_url.get(&sidecar.parent_url)
                    && seen_series_artwork.insert(series_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_SERIES_LOCAL_ARTWORK:{series_id}"),
                        priority,
                        Some(series_id.clone()),
                    ));
                }
            }
            (ScannedSidecarSource::Book, ScannedSidecarType::Metadata) => {
                if let Some(book_id) = book_by_url.get(&sidecar.parent_url)
                    && seen_books_metadata.insert(book_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_BOOK_METADATA:{book_id}"),
                        priority,
                        Some(book_id.clone()),
                    ));
                }
            }
            (ScannedSidecarSource::Book, ScannedSidecarType::Artwork) => {
                if let Some(book_id) = book_by_url.get(&sidecar.parent_url)
                    && seen_books_artwork.insert(book_id.clone())
                {
                    scheduler.enqueue(TaskQueueRecord::new(
                        format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
                        priority,
                        Some(book_id.clone()),
                    ));
                }
            }
        }
    }
}
