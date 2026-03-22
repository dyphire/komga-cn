use super::{
    BookRow, BooksListQuery, DirectBrowseBooksListFamily, DiscoveryQueries, SeriesListQuery,
    restricted_context, restricted_library_series_adapter,
};

#[test]
fn series_conditions_apply_authorized_libraries_and_restrictions() {
    let adapter = restricted_library_series_adapter();

    let use_cases = DiscoveryQueries::new(adapter);
    let context = restricted_context();

    let result = use_cases
        .list_series(
            &context,
            SeriesListQuery {
                page: 0,
                size: 20,
                library_ids: Some(vec!["lib-1".to_string(), "lib-2".to_string()]),
                deleted: None,
                oneshot: None,
                read_statuses: None,
                genres: None,
                tags: None,
                languages: None,
                publishers: None,
                age_ratings: None,
                release_dates: None,
                sharing_labels: None,
                series_statuses: None,
                complete: None,
                authors: None,
                sort: vec!["metadata.titleSort,asc".to_string()],
                search: None,
            },
        )
        .expect("series query should succeed");

    let ids: Vec<&str> = result.content.iter().map(|it| it.id.as_str()).collect();
    assert_eq!(ids, vec!["series-safe"]);
}

#[test]
fn book_conditions_apply_authorized_libraries_and_restrictions() {
    let mut adapter = restricted_library_series_adapter();
    adapter.insert_book(BookRow::new(
        "book-safe",
        "series-safe",
        "lib-1",
        "Safe Book",
    ));
    adapter.insert_book(BookRow::new(
        "book-nsfw",
        "series-nsfw",
        "lib-1",
        "Nsfw Book",
    ));
    adapter.insert_book(BookRow::new(
        "book-other-lib",
        "series-other-lib",
        "lib-2",
        "Other Book",
    ));

    let use_cases = DiscoveryQueries::new(adapter);
    let context = restricted_context();

    let result = use_cases
        .list_books(
            &context,
            BooksListQuery {
                page: 0,
                size: 20,
                unpaged: false,
                direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseSeriesPaged),
                library_ids: Some(vec!["lib-1".to_string(), "lib-2".to_string()]),
                series_ids: None,
                deleted: None,
                oneshot: None,
                tags: None,
                read_statuses: None,
                media_profiles: None,
                media_statuses: None,
                authors: None,
                release_dates: None,
                sort: vec!["metadata.title,asc".to_string()],
                search: None,
            },
        )
        .expect("books query should succeed");

    let ids: Vec<&str> = result.content.iter().map(|it| it.id.as_str()).collect();
    assert_eq!(ids, vec!["book-safe"]);
}
