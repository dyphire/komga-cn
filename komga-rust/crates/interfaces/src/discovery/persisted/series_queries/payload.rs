use crate::helpers::{normalized_date_time, normalized_file_last_modified};
use komga_domain::discovery::PageEnvelope;
use serde_json::{Value, json};

use super::super::common_helpers::{PagePayloadMetadata, page_payload};
use super::super::models::PersistedSeriesSummary;

pub(in crate::discovery) fn series_page_payload(
    page: PageEnvelope<PersistedSeriesSummary>,
    paged: bool,
    sorted: bool,
    is_admin: bool,
) -> Value {
    let content = page.content.iter().map(|series| series_payload(series, is_admin)).collect::<Vec<_>>();
    let offset = page.page.saturating_mul(page.size);

    page_payload(
        content,
        PagePayloadMetadata {
            page: page.page,
            size: page.size,
            total_elements: page.total_elements,
            total_pages: page.total_pages,
            paged,
            sorted,
            offset: if paged { offset } else { 0 },
        },
    )
}

fn series_payload(series: &PersistedSeriesSummary, is_admin: bool) -> Value {
    let url = if is_admin {
        series.name.as_str()
    } else {
        ""
    };

    let metadata = json!({
        "status": series.status.as_str(),
        "statusLock": series.status_lock,
        "title": series.title.as_str(),
        "titleLock": series.title_lock,
        "titleSort": series.title_sort.as_str(),
        "titleSortLock": series.title_sort_lock,
        "summary": series.summary.as_str(),
        "summaryLock": series.summary_lock,
        "readingDirection": series.reading_direction.as_str(),
        "readingDirectionLock": series.reading_direction_lock,
        "publisher": series.publisher.as_str(),
        "publisherLock": series.publisher_lock,
        "ageRating": series.age_rating,
        "ageRatingLock": series.age_rating_lock,
        "language": series.language.as_str(),
        "languageLock": series.language_lock,
        "genres": series.genres.clone(),
        "genresLock": series.genres_lock,
        "tags": series.tags.clone(),
        "tagsLock": series.tags_lock,
        "totalBookCount": null,
        "totalBookCountLock": series.total_book_count_lock,
        "sharingLabels": series.labels.clone(),
        "sharingLabelsLock": series.sharing_labels_lock,
        "links": series.links.iter().map(|link| json!({ "label": link.label, "url": link.url })).collect::<Vec<_>>(),
        "linksLock": series.links_lock,
        "alternateTitles": alternate_title_payloads(&series.alternate_titles),
        "alternateTitlesLock": series.alternate_titles_lock,
        "created": normalized_date_time(&series.metadata_created),
        "lastModified": normalized_date_time(&series.metadata_last_modified)
    });

    let books_metadata = json!({
        "authors": aggregated_author_payloads(&series.books_metadata_authors),
        "tags": series.books_metadata_tags.clone(),
        "releaseDate": series.books_metadata_release_date.clone(),
        "summary": series.books_metadata_summary.as_str(),
        "summaryNumber": series.books_metadata_summary_number.as_str(),
        "created": normalized_date_time(&series.books_metadata_created),
        "lastModified": normalized_date_time(&series.books_metadata_last_modified)
    });

    json!({
        "id": series.id.as_str(),
        "libraryId": series.library_id.as_str(),
        "name": series.name.as_str(),
        "url": url,
        "created": normalized_date_time(&series.created),
        "lastModified": normalized_date_time(&series.last_modified),
        "fileLastModified": normalized_file_last_modified(&series.file_last_modified),
        "booksCount": series.books_count,
        "booksReadCount": series.books_read_count,
        "booksUnreadCount": series.books_unread_count,
        "booksInProgressCount": series.books_in_progress_count,
        "metadata": metadata,
        "booksMetadata": books_metadata,
        "deleted": series.deleted,
        "oneshot": series.oneshot
    })
}

fn aggregated_author_payloads(raw: &[String]) -> Vec<Value> {
    raw.iter()
        .map(|entry| match entry.split_once("::") {
            Some((name, role)) => json!({
                "name": name,
                "role": role
            }),
            None => json!({
                "name": entry,
                "role": ""
            }),
        })
        .collect()
}

fn alternate_title_payloads(raw: &[String]) -> Vec<Value> {
    raw.iter()
        .map(|entry| match entry.split_once("::") {
            Some((label, title)) => json!({
                "label": label,
                "title": title
            }),
            None => json!({
                "label": "",
                "title": entry
            }),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_page_payload_normalizes_datetime_fields() {
        let payload = series_page_payload(
            PageEnvelope {
                content: vec![PersistedSeriesSummary {
                    id: "series-1".to_string(),
                    library_id: "library-1".to_string(),
                    name: "Series File Name".to_string(),
                    title: "Series".to_string(),
                    title_sort: "Series".to_string(),
                    labels: vec!["Team".to_string()],
                    created: "2024-01-01 00:00:00".to_string(),
                    last_modified: "2024-01-02 00:00:00".to_string(),
                    file_last_modified: "1704240000".to_string(),
                    books_count: 2,
                    books_read_count: 1,
                    books_unread_count: 1,
                    books_in_progress_count: 0,
                    status: "ONGOING".to_string(),
                    summary: "Summary".to_string(),
                    reading_direction: "LEFT_TO_RIGHT".to_string(),
                    publisher: "Publisher".to_string(),
                    age_rating: Some(13),
                    language: "en".to_string(),
                    genres: vec!["Drama".to_string()],
                    tags: vec!["Favorite".to_string()],
                    alternate_titles: vec!["Alt Title".to_string()],
                    metadata_created: "2024-01-03 00:00:00".to_string(),
                    metadata_last_modified: "2024-01-04 00:00:00".to_string(),
                    books_metadata_authors: vec!["Author,writer".to_string()],
                    books_metadata_tags: vec!["tag".to_string()],
                    books_metadata_release_date: Some("2024-01-15".to_string()),
                    books_metadata_summary: "Books summary".to_string(),
                    books_metadata_summary_number: "2".to_string(),
                    books_metadata_created: "2024-01-05 00:00:00".to_string(),
                    books_metadata_last_modified: "2024-01-06 00:00:00".to_string(),
                    deleted: false,
                    oneshot: true,
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
                    links_lock: false,
                    alternate_titles_lock: false,
                    links: vec![],
                }],
                page: 0,
                size: 20,
                total_elements: 1,
                total_pages: 1,
            },
            true,
            true,
            true,
        );

        let content = payload
            .get("content")
            .and_then(Value::as_array)
            .expect("series page payload should expose content array");
        let series = &content[0];

        assert_eq!(series.get("created"), Some(&json!("2024-01-01T00:00:00Z")));
        assert_eq!(
            series.get("lastModified"),
            Some(&json!("2024-01-02T00:00:00Z"))
        );
        assert_eq!(
            series.get("fileLastModified"),
            Some(&json!("2024-01-03T00:00:00Z"))
        );
        assert_eq!(
            series
                .get("metadata")
                .and_then(|value| value.get("created")),
            Some(&json!("2024-01-03T00:00:00Z"))
        );
        assert_eq!(
            series
                .get("booksMetadata")
                .and_then(|value| value.get("lastModified")),
            Some(&json!("2024-01-06T00:00:00Z"))
        );
    }
}
