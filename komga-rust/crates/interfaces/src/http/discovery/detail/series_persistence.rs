use super::*;

use crate::discovery_detail_access::series as series_access;

#[derive(Clone)]
pub struct PersistedSeriesResource {
    pub library_id: String,
    pub age_rating: Option<u32>,
    pub sharing_labels: Vec<String>,
}

pub struct ExistingSeriesMetadata {
    pub status: String,
    pub status_lock: bool,
    pub title: String,
    pub title_lock: bool,
    pub title_sort: String,
    pub title_sort_lock: bool,
    pub summary: String,
    pub summary_lock: bool,
    pub reading_direction: Option<String>,
    pub reading_direction_lock: bool,
    pub publisher: String,
    pub publisher_lock: bool,
    pub age_rating: Option<u32>,
    pub age_rating_lock: bool,
    pub language: String,
    pub language_lock: bool,
    pub genres: Vec<String>,
    pub genres_lock: bool,
    pub tags: Vec<String>,
    pub tags_lock: bool,
    pub total_book_count: Option<u32>,
    pub total_book_count_lock: bool,
    pub sharing_labels: Vec<String>,
    pub sharing_labels_lock: bool,
    pub links: Vec<SeriesMetadataLinkRecord>,
    pub links_lock: bool,
    pub alternate_titles: Vec<SeriesAlternateTitleRecord>,
    pub alternate_titles_lock: bool,
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

pub(super) async fn load_persisted_series_detail(
    database_file: &FsPath,
    series_id: &str,
    user_id: Option<&str>,
) -> Result<Option<SeriesDetailReadModel>, String> {
    let Some(row) = series_access::load_persisted_series_detail(database_file, series_id).await?
    else {
        return Ok(None);
    };
    let metadata = load_existing_series_metadata(database_file, series_id)
        .await?
        .unwrap_or_else(|| fallback_existing_series_metadata(&row));

    let persisted_summary = series_access::load_persisted_series_summaries(database_file)
        .await?
        .into_iter()
        .find(|entry| entry.id == series_id);

    let total_book_count = series_access::load_series_total_book_counts(database_file)
        .await?
        .get(series_id)
        .copied()
        .map(|value| value.clamp(0, i64::from(i32::MAX)) as u32);

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
        name: row.name,
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
        status_lock: metadata.status_lock,
        summary: row.summary,
        summary_lock: metadata.summary_lock,
        reading_direction: row.reading_direction,
        reading_direction_lock: metadata.reading_direction_lock,
        publisher: row.publisher,
        publisher_lock: metadata.publisher_lock,
        age_rating: row.age_rating,
        age_rating_lock: metadata.age_rating_lock,
        language: row.language,
        language_lock: metadata.language_lock,
        genres: persisted_summary
            .as_ref()
            .map(|entry| entry.genres.clone())
            .unwrap_or_default(),
        genres_lock: metadata.genres_lock,
        tags: persisted_summary
            .as_ref()
            .map(|entry| entry.tags.clone())
            .unwrap_or_default(),
        tags_lock: metadata.tags_lock,
        total_book_count,
        total_book_count_lock: metadata.total_book_count_lock,
        sharing_labels: parse_csv_values(&row.sharing_labels),
        sharing_labels_lock: metadata.sharing_labels_lock,
        links: metadata.links,
        links_lock: metadata.links_lock,
        alternate_titles: metadata.alternate_titles,
        alternate_titles_lock: metadata.alternate_titles_lock,
        title_lock: metadata.title_lock,
        title_sort_lock: metadata.title_sort_lock,
        metadata_created: row.metadata_created.clone(),
        metadata_last_modified: row.metadata_last_modified.clone(),
        books_metadata_tags: persisted_summary
            .as_ref()
            .map(|entry| entry.books_metadata_tags.clone())
            .unwrap_or_default(),
        books_metadata_authors: persisted_summary
            .as_ref()
            .map(|entry| parse_aggregated_series_authors(&entry.books_metadata_authors))
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

fn fallback_existing_series_metadata(
    row: &series_access::PersistedSeriesDetailRecord,
) -> ExistingSeriesMetadata {
    ExistingSeriesMetadata {
        status: row.status.clone(),
        status_lock: false,
        title: row.title.clone(),
        title_lock: false,
        title_sort: row.title_sort.clone(),
        title_sort_lock: false,
        summary: row.summary.clone(),
        summary_lock: false,
        reading_direction: Some(row.reading_direction.clone()).filter(|value| !value.is_empty()),
        reading_direction_lock: false,
        publisher: row.publisher.clone(),
        publisher_lock: false,
        age_rating: row.age_rating,
        age_rating_lock: false,
        language: row.language.clone(),
        language_lock: false,
        genres: vec![],
        genres_lock: false,
        tags: vec![],
        tags_lock: false,
        total_book_count: None,
        total_book_count_lock: false,
        sharing_labels: parse_csv_values(&row.sharing_labels),
        sharing_labels_lock: false,
        links: vec![],
        links_lock: false,
        alternate_titles: vec![],
        alternate_titles_lock: false,
    }
}

fn parse_aggregated_series_authors(raw: &[String]) -> Vec<BookMetadataAuthorReadModel> {
    raw.iter()
        .map(|entry| match entry.split_once("::") {
            Some((name, role)) => BookMetadataAuthorReadModel {
                name: name.to_string(),
                role: role.to_string(),
            },
            None => BookMetadataAuthorReadModel {
                name: entry.to_string(),
                role: String::new(),
            },
        })
        .collect()
}

pub(super) async fn load_persisted_series_collections(
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
            status: row.status,
            status_lock: row.status_lock,
            title: row.title,
            title_lock: row.title_lock,
            title_sort: row.title_sort,
            title_sort_lock: row.title_sort_lock,
            summary: row.summary,
            summary_lock: row.summary_lock,
            reading_direction: row.reading_direction,
            reading_direction_lock: row.reading_direction_lock,
            publisher: row.publisher,
            publisher_lock: row.publisher_lock,
            age_rating: row.age_rating,
            age_rating_lock: row.age_rating_lock,
            language: row.language,
            language_lock: row.language_lock,
            genres: row.genres,
            genres_lock: row.genres_lock,
            tags: row.tags,
            tags_lock: row.tags_lock,
            total_book_count: row.total_book_count,
            total_book_count_lock: row.total_book_count_lock,
            sharing_labels: row.sharing_labels,
            sharing_labels_lock: row.sharing_labels_lock,
            links: row.links,
            links_lock: row.links_lock,
            alternate_titles: row.alternate_titles,
            alternate_titles_lock: row.alternate_titles_lock,
        });

    Ok(metadata)
}

pub async fn persist_series_metadata_update(
    database_file: &FsPath,
    series_id: &str,
    update: SeriesMetadataUpdateRecord,
) -> Result<bool, String> {
    series_access::persist_series_metadata_update(database_file, series_id, update).await
}

pub async fn sync_series_search_documents_after_metadata_update(
    database_file: &FsPath,
    index_dir: &FsPath,
    series_id: &str,
) -> Result<(), String> {
    series_access::refresh_series_search_documents_after_metadata_update(
        database_file,
        index_dir,
        series_id,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::parse_aggregated_series_authors;

    #[test]
    fn parse_aggregated_series_authors_preserves_optional_roles() {
        let authors =
            parse_aggregated_series_authors(&["Alice::Writer".to_string(), "Bob".to_string()]);

        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0].name, "Alice");
        assert_eq!(authors[0].role, "Writer");
        assert_eq!(authors[1].name, "Bob");
        assert_eq!(authors[1].role, "");
    }
}
