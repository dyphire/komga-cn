use super::{
    BookDetailQuery, BookReadlistsQuery, BookRow, CollectionRow, DiscoveryQueries, LibraryRow,
    ReadListRow, ReadProgressRow, SeriesCollectionsQuery, SeriesDetailQuery, SeriesRow,
    SqliteDiscoveryAdapter, restricted_context,
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
    let context = restricted_context();

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
    let context = restricted_context();

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
    let context = restricted_context();

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
    let restricted_context = restricted_context();

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
