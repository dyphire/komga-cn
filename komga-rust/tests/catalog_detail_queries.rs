use komga_rust::application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, BooksListQuery, DiscoveryQueries,
    SeriesCollectionsQuery, SeriesDetailQuery,
};
use komga_rust::domain::discovery::{
    AgeRestrictionKind, DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueryContext,
    QueryRestrictions,
};
use komga_rust::persistence::discovery::{
    BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow,
    SqliteDiscoveryAdapter,
};

#[test]
fn series_detail_applies_library_and_restrictions() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_library(LibraryRow::new("lib-2", "Library 2"));
    adapter.insert_series(
        SeriesRow::new("series-safe", "lib-1", "Safe Series")
            .with_url("/library/lib-1/safe")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_series(
        SeriesRow::new("series-adult", "lib-1", "Adult Series")
            .with_url("/library/lib-1/adult")
            .with_labels(["adult"])
            .with_age_rating(18),
    );
    adapter.insert_series(
        SeriesRow::new("series-other-lib", "lib-2", "Other Library")
            .with_url("/library/lib-2/other")
            .with_labels(["safe"])
            .with_age_rating(12),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["adult".to_string()],
        }),
    };

    let safe = queries
        .get_series_detail(
            &context,
            SeriesDetailQuery {
                series_id: "series-safe".to_string(),
            },
        )
        .expect("safe series detail query should succeed");
    assert_eq!(safe.as_ref().map(|it| it.id.as_str()), Some("series-safe"));

    let restricted = queries
        .get_series_detail(
            &context,
            SeriesDetailQuery {
                series_id: "series-adult".to_string(),
            },
        )
        .expect("restricted series detail query should succeed");
    assert_eq!(restricted, None);

    let out_of_library = queries
        .get_series_detail(
            &context,
            SeriesDetailQuery {
                series_id: "series-other-lib".to_string(),
            },
        )
        .expect("out-of-library series detail query should succeed");
    assert_eq!(out_of_library, None);
}

#[test]
fn book_detail_applies_library_and_restrictions() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_library(LibraryRow::new("lib-2", "Library 2"));
    adapter.insert_series(
        SeriesRow::new("series-safe", "lib-1", "Safe Series")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_series(
        SeriesRow::new("series-adult", "lib-1", "Adult Series")
            .with_labels(["adult"])
            .with_age_rating(18),
    );
    adapter.insert_series(
        SeriesRow::new("series-other-lib", "lib-2", "Other Library").with_labels(["safe"]),
    );
    adapter.insert_book(
        BookRow::new("book-safe", "series-safe", "lib-1", "safe-book.cbz")
            .with_url("/library/lib-1/safe-book.cbz"),
    );
    adapter.insert_book(
        BookRow::new("book-adult", "series-adult", "lib-1", "adult-book.cbz")
            .with_url("/library/lib-1/adult-book.cbz"),
    );
    adapter.insert_book(
        BookRow::new(
            "book-other-lib",
            "series-other-lib",
            "lib-2",
            "other-book.cbz",
        )
        .with_url("/library/lib-2/other-book.cbz"),
    );
    adapter.insert_read_progress(
        ReadProgressRow::new("book-safe", "user-1", 9, false)
            .with_read_date("2024-01-05T01:02:03Z")
            .with_created("2024-01-05T01:02:03Z")
            .with_last_modified("2024-01-05T01:02:03Z")
            .with_device("device-kobo", "Kobo"),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["adult".to_string()],
        }),
    };

    let safe = queries
        .get_book_detail(
            &context,
            BookDetailQuery {
                book_id: "book-safe".to_string(),
            },
        )
        .expect("safe book detail query should succeed");
    assert_eq!(safe.as_ref().map(|it| it.id.as_str()), Some("book-safe"));
    assert_eq!(
        safe.as_ref()
            .and_then(|it| it.read_progress.as_ref())
            .map(|it| it.page),
        Some(9),
    );

    let restricted = queries
        .get_book_detail(
            &context,
            BookDetailQuery {
                book_id: "book-adult".to_string(),
            },
        )
        .expect("restricted book detail query should succeed");
    assert_eq!(restricted, None);

    let out_of_library = queries
        .get_book_detail(
            &context,
            BookDetailQuery {
                book_id: "book-other-lib".to_string(),
            },
        )
        .expect("out-of-library book detail query should succeed");
    assert_eq!(out_of_library, None);
}

#[test]
fn series_collections_apply_visibility_filters() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(
        SeriesRow::new("series-target", "lib-1", "Target")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_series(
        SeriesRow::new("series-adult", "lib-1", "Adult")
            .with_labels(["adult"])
            .with_age_rating(18),
    );
    adapter.insert_collection(
        CollectionRow::new("collection-clean", "Collection Clean")
            .with_series_ids(["series-target"]),
    );
    adapter.insert_collection(
        CollectionRow::new("collection-mixed", "Collection Mixed")
            .with_series_ids(["series-target", "series-adult"]),
    );
    adapter.insert_collection(
        CollectionRow::new("collection-adult-only", "Collection Adult Only")
            .with_series_ids(["series-adult"]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["adult".to_string()],
        }),
    };

    let collections = queries
        .list_series_collections(
            &context,
            SeriesCollectionsQuery {
                series_id: "series-target".to_string(),
            },
        )
        .expect("series collections query should succeed");

    assert_eq!(
        collections
            .iter()
            .map(|it| it.id.as_str())
            .collect::<Vec<_>>(),
        vec!["collection-clean", "collection-mixed"],
    );
    assert_eq!(collections[0].series_ids, vec!["series-target".to_string()]);
    assert_eq!(collections[0].filtered, false);
    assert_eq!(collections[1].series_ids, vec!["series-target".to_string()]);
    assert_eq!(collections[1].filtered, true);
}

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
fn book_readlists_apply_visibility_filters() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(
        SeriesRow::new("series-safe", "lib-1", "Safe Series")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_series(
        SeriesRow::new("series-adult", "lib-1", "Adult Series")
            .with_labels(["adult"])
            .with_age_rating(18),
    );
    adapter.insert_book(BookRow::new(
        "book-safe",
        "series-safe",
        "lib-1",
        "Book Safe",
    ));
    adapter.insert_book(BookRow::new(
        "book-adult",
        "series-adult",
        "lib-1",
        "Book Adult",
    ));
    adapter.insert_read_list(
        ReadListRow::new("readlist-clean", "ReadList Clean").with_book_ids(["book-safe"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-mixed", "ReadList Mixed")
            .with_book_ids(["book-safe", "book-adult"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-denied", "ReadList Denied").with_book_ids(["book-adult"]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let restricted_context = DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["adult".to_string()],
        }),
    };

    let readlists = queries
        .list_book_readlists(
            &restricted_context,
            BookReadlistsQuery {
                book_id: "book-safe".to_string(),
            },
        )
        .expect("book readlists query should succeed");

    assert_eq!(
        readlists
            .iter()
            .map(|it| it.id.as_str())
            .collect::<Vec<_>>(),
        vec!["readlist-clean", "readlist-mixed"],
    );
    assert_eq!(readlists[0].filtered, false);
    assert_eq!(readlists[0].book_ids, vec!["book-safe".to_string()]);
    assert_eq!(readlists[1].filtered, true);
    assert_eq!(readlists[1].book_ids, vec!["book-safe".to_string()]);
}
