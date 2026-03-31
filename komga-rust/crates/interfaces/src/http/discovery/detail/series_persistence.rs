use super::*;

use crate::discovery_detail_access::series as series_access;

#[derive(Clone)]
pub struct PersistedSeriesResource {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
}

pub struct ExistingSeriesMetadata {
    pub title: String,
    pub title_sort: String,
    pub summary: String,
}

pub async fn load_persisted_series_resource(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedSeriesResource>, String> {
    let resource = series_access::load_persisted_series_resource(database_file, series_id)
        .await?
        .map(|row| PersistedSeriesResource {
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: parse_csv_values(&row.sharing_labels),
        });

    Ok(resource)
}

pub async fn resolve_series_id_for_persisted(
    database_file: &FsPath,
    requested_series_id: &str,
) -> String {
    let Some(index) = requested_series_id
        .strip_prefix("series-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_series_id.to_string();
    };

    if index == 0 {
        return requested_series_id.to_string();
    }

    if matches!(
        load_persisted_series_resource(database_file, requested_series_id).await,
        Ok(Some(_))
    ) {
        return requested_series_id.to_string();
    }

    match series_access::load_series_id_by_sorted_position(database_file, index).await {
        Ok(Some(series_id)) => series_id,
        _ => requested_series_id.to_string(),
    }
}

pub async fn load_persisted_series_detail(
    database_file: &FsPath,
    series_id: &str,
    user_id: Option<&str>,
) -> Result<Option<SeriesDetailReadModel>, String> {
    let Some(row) = series_access::load_persisted_series_detail(database_file, series_id).await?
    else {
        return Ok(None);
    };

    let persisted_summary = series_access::load_persisted_series_summaries(database_file)
        .await?
        .into_iter()
        .find(|entry| entry.id == series_id);

    let total_book_count = series_access::load_series_total_book_counts(database_file)
        .await?
        .get(series_id)
        .copied()
        .map(|value| value.clamp(0, i64::from(u32::MAX)) as u32);

    let (books_read_count, books_in_progress_count) = if let Some(user_id) = user_id {
        let counts = series_access::load_series_read_progress_counts(database_file, user_id)
            .await?
            .get(series_id)
            .copied();
        let read = counts
            .map(|(read, _)| read.clamp(0, i64::from(u32::MAX)) as u32)
            .unwrap_or(0);
        let in_progress = counts
            .map(|(_, in_progress)| in_progress.clamp(0, i64::from(u32::MAX)) as u32)
            .unwrap_or(0);
        (read, in_progress)
    } else {
        (0, 0)
    };

    let books_unread_count = row
        .books_count
        .saturating_sub(books_read_count.saturating_add(books_in_progress_count));

    let model = Some(SeriesDetailReadModel {
        id: row.id,
        library_id: row.library_id,
        title: row.title,
        title_sort: row.title_sort,
        url: row.url,
        created: row.created,
        last_modified: row.last_modified,
        file_last_modified: row.file_last_modified,
        books_count: row.books_count,
        books_read_count,
        books_unread_count,
        books_in_progress_count,
        status: row.status,
        summary: row.summary,
        reading_direction: row.reading_direction,
        publisher: row.publisher,
        age_rating: row.age_rating,
        language: row.language,
        genres: persisted_summary
            .as_ref()
            .map(|entry| entry.genres.clone())
            .unwrap_or_default(),
        tags: persisted_summary
            .as_ref()
            .map(|entry| entry.tags.clone())
            .unwrap_or_default(),
        total_book_count,
        sharing_labels: parse_csv_values(&row.sharing_labels),
        alternate_titles: persisted_summary
            .as_ref()
            .map(|entry| entry.alternate_titles.clone())
            .unwrap_or_default(),
        metadata_created: row.metadata_created.clone(),
        metadata_last_modified: row.metadata_last_modified.clone(),
        books_metadata_tags: persisted_summary
            .as_ref()
            .map(|entry| entry.books_metadata_tags.clone())
            .unwrap_or_default(),
        books_metadata_release_date: persisted_summary
            .as_ref()
            .and_then(|entry| entry.books_metadata_release_date.clone()),
        books_metadata_summary: persisted_summary
            .as_ref()
            .map(|entry| entry.books_metadata_summary.clone())
            .unwrap_or_default(),
        books_metadata_summary_number: persisted_summary
            .as_ref()
            .map(|entry| entry.books_metadata_summary_number.clone())
            .unwrap_or_default(),
        books_metadata_created: persisted_summary
            .as_ref()
            .map(|entry| entry.books_metadata_created.clone())
            .unwrap_or_else(|| row.metadata_created.clone()),
        books_metadata_last_modified: persisted_summary
            .as_ref()
            .map(|entry| entry.books_metadata_last_modified.clone())
            .unwrap_or_else(|| row.metadata_last_modified.clone()),
        deleted: row.deleted,
        oneshot: row.oneshot,
    });

    Ok(model)
}

pub async fn load_persisted_series_collections(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<CollectionReadModel>, String> {
    let rows = series_access::load_persisted_series_collections(database_file, series_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| CollectionReadModel {
            id: row.id,
            name: row.name,
            ordered: row.ordered,
            series_ids: row.series_ids,
            created_date: row.created_date,
            last_modified_date: row.last_modified_date,
            filtered: false,
        })
        .collect())
}

pub async fn load_existing_series_metadata(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<ExistingSeriesMetadata>, String> {
    let metadata = series_access::load_existing_series_metadata(database_file, series_id)
        .await?
        .map(|row| ExistingSeriesMetadata {
            title: row.title,
            title_sort: row.title_sort,
            summary: row.summary,
        });

    Ok(metadata)
}

pub async fn persist_series_metadata_update(
    database_file: &FsPath,
    series_id: &str,
    title: &str,
    title_sort: &str,
    summary: &str,
) -> Result<bool, String> {
    series_access::persist_series_metadata_update(
        database_file,
        series_id,
        title,
        title_sort,
        summary,
    )
    .await
}

pub async fn refresh_series_search_document(
    database_file: &FsPath,
    series_id: &str,
) -> Result<(), String> {
    series_access::refresh_series_after_metadata_update(database_file, series_id).await
}
