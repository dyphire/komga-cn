use komga_application::discovery::{
    BookMetadataAuthorReadModel, CollectionReadModel, SeriesAlternateTitleRecord,
    SeriesMetadataLinkRecord,
};

use super::SeriesDetailReadModel;
use super::detail_utils::parse_group_concat_values;
use crate::state::DiscoveryState;

#[derive(Clone)]
pub(in crate::discovery) struct PersistedSeriesResource {
    pub(in crate::discovery) library_id: String,
    pub(in crate::discovery) age_rating: Option<u32>,
    pub(in crate::discovery) sharing_labels: Vec<String>,
}

struct ExistingSeriesMetadata {
    status_lock: bool,
    title_lock: bool,
    title_sort_lock: bool,
    summary_lock: bool,
    reading_direction_lock: bool,
    publisher_lock: bool,
    age_rating_lock: bool,
    language_lock: bool,
    genres_lock: bool,
    tags_lock: bool,
    total_book_count_lock: bool,
    sharing_labels_lock: bool,
    links: Vec<SeriesMetadataLinkRecord>,
    links_lock: bool,
    alternate_titles: Vec<SeriesAlternateTitleRecord>,
    alternate_titles_lock: bool,
}

#[derive(Default)]
struct SeriesDetailReadProgressCounts {
    read: u32,
    in_progress: u32,
}

impl SeriesDetailReadProgressCounts {
    fn unread_count(&self, books_count: u32) -> u32 {
        books_count.saturating_sub(self.read.saturating_add(self.in_progress))
    }
}

pub(in crate::discovery) async fn load_persisted_series_resource(
    app: &DiscoveryState,
    series_id: &str,
) -> Result<Option<PersistedSeriesResource>, String> {
    let resource = app
        .series_detail
        .load_persisted_series_resource(series_id)
        .await?
        .map(|row| PersistedSeriesResource {
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: parse_group_concat_values(&row.sharing_labels),
        });

    Ok(resource)
}

pub(super) async fn load_persisted_series_detail(
    app: &DiscoveryState,
    series_id: &str,
    user_id: Option<&str>,
) -> Result<Option<SeriesDetailReadModel>, String> {
    let Some(row) = app
        .series_detail
        .load_persisted_series_detail(series_id)
        .await?
    else {
        return Ok(None);
    };
    let metadata = load_existing_series_metadata(app, series_id)
        .await?
        .unwrap_or_else(fallback_existing_series_metadata);

    let persisted_summary = app
        .series_detail
        .load_persisted_series_summaries()
        .await?
        .into_iter()
        .find(|entry| entry.id == series_id);

    let total_book_count = app
        .series_detail
        .load_series_total_book_counts()
        .await?
        .get(series_id)
        .copied()
        .map(|value| value.clamp(0, i64::from(i32::MAX)) as u32);

    let read_progress_counts = if let Some(user_id) = user_id {
        let counts = app
            .series_detail
            .load_series_read_progress_counts(user_id)
            .await?
            .get(series_id)
            .copied();
        SeriesDetailReadProgressCounts {
            read: counts
                .map(|counts| counts.read_count.clamp(0, i64::from(u32::MAX)) as u32)
                .unwrap_or(0),
            in_progress: counts
                .map(|counts| counts.in_progress_count.clamp(0, i64::from(u32::MAX)) as u32)
                .unwrap_or(0),
        }
    } else {
        SeriesDetailReadProgressCounts::default()
    };

    let books_unread_count = read_progress_counts.unread_count(row.books_count);

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
        books_read_count: read_progress_counts.read,
        books_unread_count,
        books_in_progress_count: read_progress_counts.in_progress,
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
        sharing_labels: parse_group_concat_values(&row.sharing_labels),
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

fn fallback_existing_series_metadata() -> ExistingSeriesMetadata {
    ExistingSeriesMetadata {
        status_lock: false,
        title_lock: false,
        title_sort_lock: false,
        summary_lock: false,
        reading_direction_lock: false,
        publisher_lock: false,
        age_rating_lock: false,
        language_lock: false,
        genres_lock: false,
        tags_lock: false,
        total_book_count_lock: false,
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
    app: &DiscoveryState,
    series_id: &str,
) -> Result<Vec<CollectionReadModel>, String> {
    let rows = app
        .series_detail
        .load_persisted_series_collections(series_id)
        .await?;
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

async fn load_existing_series_metadata(
    app: &DiscoveryState,
    series_id: &str,
) -> Result<Option<ExistingSeriesMetadata>, String> {
    let metadata = app
        .series_detail
        .load_existing_series_metadata(series_id)
        .await?
        .map(|row| ExistingSeriesMetadata {
            status_lock: row.status_lock,
            title_lock: row.title_lock,
            title_sort_lock: row.title_sort_lock,
            summary_lock: row.summary_lock,
            reading_direction_lock: row.reading_direction_lock,
            publisher_lock: row.publisher_lock,
            age_rating_lock: row.age_rating_lock,
            language_lock: row.language_lock,
            genres_lock: row.genres_lock,
            tags_lock: row.tags_lock,
            total_book_count_lock: row.total_book_count_lock,
            sharing_labels_lock: row.sharing_labels_lock,
            links: row.links,
            links_lock: row.links_lock,
            alternate_titles: row.alternate_titles,
            alternate_titles_lock: row.alternate_titles_lock,
        });

    Ok(metadata)
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
