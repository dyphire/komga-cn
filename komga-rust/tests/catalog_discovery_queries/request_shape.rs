use super::{
    BooksListQuery, DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueries,
    DiscoveryQueryContext, LibraryListQuery, SeriesListQuery, SqliteDiscoveryAdapter,
};

#[test]
fn unsupported_sorts_are_classified_non_native() {
    let use_cases = DiscoveryQueries::new(SqliteDiscoveryAdapter::default());
    let context = DiscoveryQueryContext::allow_all();

    let series = use_cases.list_series(
        &context,
        SeriesListQuery {
            page: 0,
            size: 20,
            library_ids: None,
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
            sort: vec!["random,asc".to_string()],
            search: None,
        },
    );
    let books = use_cases.list_books(
        &context,
        BooksListQuery {
            page: 0,
            size: 20,
            unpaged: false,
            direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseSeriesPaged),
            library_ids: None,
            series_ids: None,
            deleted: None,
            oneshot: None,
            tags: None,
            read_statuses: None,
            media_profiles: None,
            media_statuses: None,
            authors: None,
            release_dates: None,
            sort: vec!["readProgress.readDate,desc".to_string()],
            search: None,
        },
    );

    assert!(matches!(
        series,
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));
    assert!(matches!(
        books,
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));

    let _ = use_cases
        .list_libraries(&context, LibraryListQuery {})
        .expect("libraries should not require sort classification");
}
