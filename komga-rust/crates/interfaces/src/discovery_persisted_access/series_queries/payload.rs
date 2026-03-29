use super::*;

pub fn series_page_payload(page: PageEnvelope<PersistedSeriesSummary>, paged: bool) -> Value {
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
                "empty": false,
                "sorted": true,
                "unsorted": false
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
            "empty": false,
            "sorted": true,
            "unsorted": false
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
        "created": series.metadata_created.as_str(),
        "lastModified": series.metadata_last_modified.as_str()
    });

    let books_metadata = json!({
        "authors": series.books_metadata_authors.clone(),
        "tags": series.books_metadata_tags.clone(),
        "releaseDate": series.books_metadata_release_date.clone(),
        "summary": series.books_metadata_summary.as_str(),
        "summaryNumber": series.books_metadata_summary_number.as_str(),
        "created": series.books_metadata_created.as_str(),
        "lastModified": series.books_metadata_last_modified.as_str()
    });

    json!({
        "id": series.id.as_str(),
        "libraryId": series.library_id.as_str(),
        "name": series.title.as_str(),
        "url": format!("series/{}", series.id),
        "created": series.created.as_str(),
        "lastModified": series.last_modified.as_str(),
        "fileLastModified": series.file_last_modified.as_str(),
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
