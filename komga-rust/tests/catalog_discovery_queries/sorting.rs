use super::{
    BookRow, BooksListQuery, DiscoveryQueries, DiscoveryQueryContext, LibraryRow, SeriesRow,
    SqliteDiscoveryAdapter,
};

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
