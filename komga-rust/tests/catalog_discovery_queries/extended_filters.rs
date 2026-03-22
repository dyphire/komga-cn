use super::{
    BookRow, BooksListQuery, DirectBrowseBooksListFamily, DiscoveryQueries, DiscoveryQueryContext,
    LibraryRow, SeriesListQuery, SeriesRow, SqliteDiscoveryAdapter,
};

#[test]
fn series_t1_extended_filters_are_applied_in_query_layer() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));

    adapter.insert_series(
        SeriesRow::new("series-match", "lib-1", "Series Match")
            .with_labels(["safe"])
            .with_genres(["fantasy"])
            .with_tags(["featured"])
            .with_language("en")
            .with_publisher("komga")
            .with_age_rating(16)
            .with_release_date("2024-01-01")
            .with_status("ONGOING")
            .with_complete(true)
            .with_read_status("READ")
            .with_authors(["Alice"]),
    );

    adapter.insert_series(
        SeriesRow::new("series-other", "lib-1", "Series Other")
            .with_labels(["safe"])
            .with_genres(["mystery"])
            .with_tags(["other"])
            .with_language("fr")
            .with_publisher("other")
            .with_age_rating(10)
            .with_release_date("2023-01-01")
            .with_status("ENDED")
            .with_complete(false)
            .with_read_status("UNREAD")
            .with_authors(["Bob"]),
    );

    let use_cases = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext::allow_all();

    let result = use_cases
        .list_series(
            &context,
            SeriesListQuery {
                page: 0,
                size: 20,
                library_ids: Some(vec!["lib-1".to_string()]),
                deleted: None,
                oneshot: None,
                read_statuses: Some(vec!["READ".to_string()]),
                genres: Some(vec!["fantasy".to_string()]),
                tags: Some(vec!["featured".to_string()]),
                languages: Some(vec!["en".to_string()]),
                publishers: Some(vec!["komga".to_string()]),
                age_ratings: Some(vec![16]),
                release_dates: Some(vec!["2024-01-01".to_string()]),
                sharing_labels: Some(vec!["safe".to_string()]),
                series_statuses: Some(vec!["ONGOING".to_string()]),
                complete: Some(true),
                authors: Some(vec!["alice".to_string()]),
                sort: vec!["metadata.titleSort,asc".to_string()],
                search: None,
            },
        )
        .expect("series query should succeed");

    let ids: Vec<&str> = result.content.iter().map(|it| it.id.as_str()).collect();
    assert_eq!(ids, vec!["series-match"]);
}

#[test]
fn books_t1_extended_filters_are_applied_in_query_layer() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(SeriesRow::new("series-1", "lib-1", "Series 1").with_labels(["safe"]));

    adapter.insert_book(
        BookRow::new("book-match", "series-1", "lib-1", "Book Match")
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_release_date("2024-01-01")
            .with_read_status("READ")
            .with_authors(["Alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-other", "series-1", "lib-1", "Book Other")
            .with_media("ERROR", "application/zip", 1)
            .with_media_profile("PROFILE-2")
            .with_release_date("2023-01-01")
            .with_read_status("UNREAD")
            .with_authors(["Bob"]),
    );

    let use_cases = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext::allow_all();

    let result = use_cases
        .list_books(
            &context,
            BooksListQuery {
                page: 0,
                size: 20,
                unpaged: false,
                direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseSeriesPaged),
                library_ids: Some(vec!["lib-1".to_string()]),
                series_ids: Some(vec!["series-1".to_string()]),
                deleted: None,
                oneshot: None,
                tags: None,
                read_statuses: Some(vec!["READ".to_string()]),
                media_profiles: Some(vec!["PROFILE-1".to_string()]),
                media_statuses: Some(vec!["READY".to_string()]),
                authors: Some(vec!["alice".to_string()]),
                release_dates: Some(vec!["2024-01-01".to_string()]),
                sort: vec!["metadata.title,asc".to_string()],
                search: None,
            },
        )
        .expect("books query should succeed");

    let ids: Vec<&str> = result.content.iter().map(|it| it.id.as_str()).collect();
    assert_eq!(ids, vec!["book-match"]);
}
