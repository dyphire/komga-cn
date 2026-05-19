use crate::discovery::persisted::models::PersistedSeriesSummary;
use crate::discovery::persisted::series_queries::series_page_payload;
use komga_application::discovery::SeriesReadModel;
use komga_domain::discovery::PageEnvelope;
use serde_json::Value;

fn series_read_model_to_persisted(model: &SeriesReadModel) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: model.id.clone(),
        library_id: model.library_id.clone(),
        name: model.name.clone(),
        title: model.title.clone(),
        title_sort: model.title_sort.clone(),
        labels: model.labels.clone(),
        created: model.created.clone(),
        last_modified: model.last_modified.clone(),
        file_last_modified: model.file_last_modified.clone(),
        books_count: model.books_count,
        books_read_count: model.books_read_count,
        books_unread_count: model.books_unread_count,
        books_in_progress_count: model.books_in_progress_count,
        status: model.status.clone(),
        summary: model.summary.clone(),
        reading_direction: model.reading_direction.clone(),
        publisher: model.publisher.clone(),
        age_rating: model.age_rating,
        language: model.language.clone(),
        genres: model.genres.clone(),
        tags: model.tags.clone(),
        alternate_titles: model.alternate_titles.clone(),
        metadata_created: model.metadata_created.clone(),
        metadata_last_modified: model.metadata_last_modified.clone(),
        books_metadata_authors: model.books_metadata_authors.clone(),
        books_metadata_tags: model.books_metadata_tags.clone(),
        books_metadata_release_date: model.books_metadata_release_date.clone(),
        books_metadata_summary: model.books_metadata_summary.clone(),
        books_metadata_summary_number: model.books_metadata_summary_number.clone(),
        books_metadata_created: model.books_metadata_created.clone(),
        books_metadata_last_modified: model.books_metadata_last_modified.clone(),
        deleted: model.deleted,
        oneshot: model.oneshot,
    }
}

pub(in crate::discovery) fn series_read_model_page_payload(
    page: PageEnvelope<SeriesReadModel>,
    paged: bool,
    sorted: bool,
) -> Value {
    let converted = PageEnvelope {
        content: page
            .content
            .iter()
            .map(series_read_model_to_persisted)
            .collect(),
        page: page.page,
        size: page.size,
        total_elements: page.total_elements,
        total_pages: page.total_pages,
    };
    series_page_payload(converted, paged, sorted)
}
