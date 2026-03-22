use super::{
    BookRow, BookSiblingQuery, BooksListQuery, DirectBrowseBooksListFamily, DiscoveryError,
    DiscoveryQueries, DiscoveryQueryContext, LibraryRow, ReadListRow, SeriesRow,
    SqliteDiscoveryAdapter,
};

#[test]
fn page_scoped_books_list_rejects_extra_filters_and_sorts() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_series(SeriesRow::new("series-1", "1", "Series 1").with_labels(["safe"]));
    adapter.insert_book(BookRow::new("book-1", "series-1", "1", "Book 1").with_number_sort(1));

    let use_cases = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext::allow_all();
    let base = BooksListQuery {
        page: 0,
        size: 20,
        unpaged: false,
        direct_browse_family: Some(DirectBrowseBooksListFamily::BrowseSeriesPaged),
        library_ids: None,
        series_ids: Some(vec!["series-1".to_string()]),
        deleted: None,
        oneshot: None,
        tags: None,
        read_statuses: None,
        media_profiles: None,
        media_statuses: None,
        authors: None,
        release_dates: None,
        sort: vec!["metadata.numberSort,asc".to_string()],
        search: None,
    };

    let ok = use_cases
        .list_books_direct_browse(&context, base.clone())
        .expect("page-scoped direct browse shape should be accepted");
    assert_eq!(ok.total_elements, 1);

    let with_extra_filter = BooksListQuery {
        read_statuses: Some(vec!["READ".to_string()]),
        ..base.clone()
    };
    assert!(matches!(
        use_cases.list_books_direct_browse(&context, with_extra_filter),
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));

    let with_alternate_sort = BooksListQuery {
        sort: vec!["readProgress.readDate,desc".to_string()],
        ..base
    };
    assert!(matches!(
        use_cases.list_books_direct_browse(&context, with_alternate_sort),
        Err(DiscoveryError::NonNativeRequestShape(_))
    ));
}

#[test]
fn sibling_navigation_uses_number_sort_seek() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(SeriesRow::new("series-1", "lib-1", "Series 1").with_labels(["safe"]));
    adapter.insert_book(
        BookRow::new("book-1", "series-1", "lib-1", "Book 1")
            .with_number_sort(1)
            .with_url("/library/lib-1/book-1.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-2", "series-1", "lib-1", "Book 2")
            .with_number_sort(5)
            .with_url("/library/lib-1/book-2.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-3", "series-1", "lib-1", "Book 3")
            .with_number_sort(10)
            .with_url("/library/lib-1/book-3.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-other", "series-1", "lib-1", "Book Other")
            .with_number_sort(100)
            .with_deleted(true)
            .with_url("/library/lib-1/book-other.cbz"),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: None,
    };

    let previous = queries
        .get_book_sibling_previous(
            &context,
            BookSiblingQuery {
                book_id: "book-2".to_string(),
            },
        )
        .expect("book previous sibling query should succeed")
        .expect("book previous sibling should exist");
    assert_eq!(previous.id, "book-1");

    let next = queries
        .get_book_sibling_next(
            &context,
            BookSiblingQuery {
                book_id: "book-2".to_string(),
            },
        )
        .expect("book next sibling query should succeed")
        .expect("book next sibling should exist");
    assert_eq!(next.id, "book-3");

    let no_previous = queries
        .get_book_sibling_previous(
            &context,
            BookSiblingQuery {
                book_id: "book-1".to_string(),
            },
        )
        .expect("book previous boundary query should succeed");
    assert_eq!(no_previous, None);
}

#[test]
fn readlist_sibling_navigation_uses_visible_readlist_order() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(
        SeriesRow::new("series-safe", "lib-1", "Series Safe")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_series(
        SeriesRow::new("series-hidden", "lib-1", "Series Hidden")
            .with_labels(["adult"])
            .with_age_rating(18),
    );
    adapter.insert_book(
        BookRow::new("book-1", "series-safe", "lib-1", "Book 1")
            .with_release_date("2020-01-01")
            .with_url("/library/lib-1/book-1.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-2", "series-hidden", "lib-1", "Book 2")
            .with_release_date("2020-01-02")
            .with_url("/library/lib-1/book-2.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-3", "series-safe", "lib-1", "Book 3")
            .with_release_date("2020-01-03")
            .with_url("/library/lib-1/book-3.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-4", "series-safe", "lib-1", "Book 4")
            .with_release_date("2020-01-04")
            .with_url("/library/lib-1/book-4.cbz"),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-ordered", "ReadList Ordered")
            .with_ordered(true)
            .with_book_ids(["book-1", "book-2", "book-3", "book-4"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-unordered", "ReadList Unordered")
            .with_ordered(false)
            .with_book_ids(["book-4", "book-2", "book-1", "book-3"]),
    );

    let restricted_context = super::restricted_context();

    let ordered_next_hidden_middle = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-ordered", "book-1")
        .expect("ordered hidden-middle next query should succeed")
        .expect("ordered hidden-middle next sibling should exist");
    assert_eq!(ordered_next_hidden_middle.id, "book-3");

    let ordered_prev_hidden_middle = adapter
        .get_readlist_book_sibling_previous(&restricted_context, "readlist-ordered", "book-3")
        .expect("ordered hidden-middle previous query should succeed")
        .expect("ordered hidden-middle previous sibling should exist");
    assert_eq!(ordered_prev_hidden_middle.id, "book-1");

    let ordered_first_previous = adapter
        .get_readlist_book_sibling_previous(&restricted_context, "readlist-ordered", "book-1")
        .expect("ordered first previous boundary query should succeed");
    assert_eq!(ordered_first_previous, None);

    let ordered_last_next = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-ordered", "book-4")
        .expect("ordered last next boundary query should succeed");
    assert_eq!(ordered_last_next, None);

    let ordered_hidden_anchor = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-ordered", "book-2")
        .expect("ordered hidden anchor query should succeed");
    assert_eq!(ordered_hidden_anchor, None);

    let ordered_non_member = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-ordered", "book-out")
        .expect("ordered non-member query should succeed");
    assert_eq!(ordered_non_member, None);

    let unordered_next = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-unordered", "book-1")
        .expect("unordered next query should succeed")
        .expect("unordered next sibling should exist");
    assert_eq!(unordered_next.id, "book-3");

    let unordered_previous = adapter
        .get_readlist_book_sibling_previous(&restricted_context, "readlist-unordered", "book-3")
        .expect("unordered previous query should succeed")
        .expect("unordered previous sibling should exist");
    assert_eq!(unordered_previous.id, "book-1");

    let unordered_first_previous = adapter
        .get_readlist_book_sibling_previous(&restricted_context, "readlist-unordered", "book-1")
        .expect("unordered first previous boundary query should succeed");
    assert_eq!(unordered_first_previous, None);

    let unordered_last_next = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-unordered", "book-4")
        .expect("unordered last next boundary query should succeed");
    assert_eq!(unordered_last_next, None);

    let unordered_non_member = adapter
        .get_readlist_book_sibling_next(&restricted_context, "readlist-unordered", "book-out")
        .expect("unordered non-member query should succeed");
    assert_eq!(unordered_non_member, None);
}
