use super::*;

use crate::http::helpers::{normalized_date_time, normalized_file_last_modified};

pub fn series_page_payload(
    page: PageEnvelope<PersistedSeriesSummary>,
    paged: bool,
    sorted: bool,
) -> Value {
    let content = page.content.iter().map(series_payload).collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = page.page.saturating_mul(page.size);

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": !sorted,
                "sorted": sorted,
                "unsorted": !sorted
            },
            "offset": offset,
            "paged": paged,
            "unpaged": !paged
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": !sorted,
            "sorted": sorted,
            "unsorted": !sorted
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn series_payload(series: &PersistedSeriesSummary) -> Value {
    let metadata = json!({
        "status": series.status.as_str(),
        "statusLock": false,
        "title": series.title.as_str(),
        "titleLock": false,
        "titleSort": series.title_sort.as_str(),
        "titleSortLock": false,
        "summary": series.summary.as_str(),
        "summaryLock": false,
        "readingDirection": series.reading_direction.as_str(),
        "readingDirectionLock": false,
        "publisher": series.publisher.as_str(),
        "publisherLock": false,
        "ageRating": series.age_rating,
        "ageRatingLock": false,
        "language": series.language.as_str(),
        "languageLock": false,
        "genres": series.genres.clone(),
        "genresLock": false,
        "tags": series.tags.clone(),
        "tagsLock": false,
        "totalBookCount": null,
        "totalBookCountLock": false,
        "sharingLabels": series.labels.clone(),
        "sharingLabelsLock": false,
        "links": [],
        "linksLock": false,
        "alternateTitles": series.alternate_titles.clone(),
        "alternateTitlesLock": false,
        "created": normalized_date_time(&series.metadata_created),
        "lastModified": normalized_date_time(&series.metadata_last_modified)
    });

    let books_metadata = json!({
        "authors": series.books_metadata_authors.clone(),
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
        "name": series.title.as_str(),
        "url": format!("series/{}", series.id),
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
                }],
                page: 0,
                size: 20,
                total_elements: 1,
                total_pages: 1,
            },
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
