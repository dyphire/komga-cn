use komga_rust::application::discovery::{
    BooksListQuery, DiscoveryQueries, LibraryListQuery, SeriesListQuery,
};
use komga_rust::domain::discovery::{
    AgeRestrictionKind, DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueryContext,
    QueryRestrictions,
};
use komga_rust::persistence::discovery::{BookRow, LibraryRow, SeriesRow, SqliteDiscoveryAdapter};

#[test]
fn series_conditions_apply_authorized_libraries_and_restrictions() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_library(LibraryRow::new("lib-2", "Library 2"));
    adapter
        .insert_series(SeriesRow::new("series-safe", "lib-1", "Safe Series").with_labels(["safe"]));
    adapter
        .insert_series(SeriesRow::new("series-nsfw", "lib-1", "Nsfw Series").with_labels(["nsfw"]));
    adapter.insert_series(
        SeriesRow::new("series-other-lib", "lib-2", "Other Library").with_labels(["safe"]),
    );

    let use_cases = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["nsfw".to_string()],
        }),
    };

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
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_library(LibraryRow::new("lib-2", "Library 2"));
    adapter
        .insert_series(SeriesRow::new("series-safe", "lib-1", "Safe Series").with_labels(["safe"]));
    adapter
        .insert_series(SeriesRow::new("series-nsfw", "lib-1", "Nsfw Series").with_labels(["nsfw"]));
    adapter.insert_series(
        SeriesRow::new("series-other-lib", "lib-2", "Other Library").with_labels(["safe"]),
    );
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
    let context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["nsfw".to_string()],
        }),
    };

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

#[test]
fn books_list_honors_metadata_title_sort_for_generic_discovery() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(SeriesRow::new("series-1", "lib-1", "Series 1").with_labels(["safe"]));

    adapter.insert_book(
        BookRow::new("book-b", "series-1", "lib-1", "B Book")
            .with_number_sort(1)
            .with_last_modified("2024-01-02T03:04:05Z"),
    );
    adapter.insert_book(
        BookRow::new("book-a", "series-1", "lib-1", "A Book")
            .with_number_sort(2)
            .with_last_modified("2024-01-02T03:04:06Z"),
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
                direct_browse_family: None,
                library_ids: Some(vec!["lib-1".to_string()]),
                series_ids: Some(vec!["series-1".to_string()]),
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
    assert_eq!(ids, vec!["book-a", "book-b"]);
}
