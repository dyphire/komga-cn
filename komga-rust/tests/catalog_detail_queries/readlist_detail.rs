use super::{
    BookReadlistsQuery, BookRow, DiscoveryQueries, LibraryRow, ReadListDetailQuery, ReadListRow,
    SeriesRow, SqliteDiscoveryAdapter, restricted_context,
};

pub(super) async fn phase6_readlist_detail_visible_filtered_and_not_found_parity() {
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
        SeriesRow::new("series-lib2", "lib-2", "Library 2 Series")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_book(BookRow::new("book-safe", "series-safe", "lib-1", "Book Safe"));
    adapter.insert_book(BookRow::new(
        "book-adult",
        "series-adult",
        "lib-1",
        "Book Adult",
    ));
    adapter.insert_book(BookRow::new(
        "book-lib2",
        "series-lib2",
        "lib-2",
        "Book Library 2",
    ));

    adapter.insert_read_list(
        ReadListRow::new("readlist-visible", "ReadList Visible")
            .with_summary("visible")
            .with_book_ids(["book-safe"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-filtered", "ReadList Filtered")
            .with_summary("filtered")
            .with_book_ids(["book-safe", "book-adult"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-lib2", "ReadList Library 2")
            .with_summary("hidden")
            .with_book_ids(["book-lib2"]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = restricted_context();

    let visible = queries
        .get_readlist_detail(
            &context,
            ReadListDetailQuery {
                readlist_id: "readlist-visible".to_string(),
            },
        )
        .await
        .expect("visible readlist detail should succeed")
        .expect("visible readlist should remain accessible");
    assert_eq!(visible.id, "readlist-visible");
    assert_eq!(visible.book_ids, vec!["book-safe"]);
    assert!(!visible.filtered);

    let filtered = queries
        .get_readlist_detail(
            &context,
            ReadListDetailQuery {
                readlist_id: "readlist-filtered".to_string(),
            },
        )
        .await
        .expect("filtered readlist detail should succeed")
        .expect("partially visible readlist should remain accessible");
    assert_eq!(filtered.id, "readlist-filtered");
    assert_eq!(filtered.book_ids, vec!["book-safe"]);
    assert!(filtered.filtered);

    let fully_inaccessible = queries
        .get_readlist_detail(
            &context,
            ReadListDetailQuery {
                readlist_id: "readlist-lib2".to_string(),
            },
        )
        .await
        .expect("fully inaccessible readlist lookup should succeed");
    assert!(
        fully_inaccessible.is_none(),
        "fully inaccessible readlist must be hidden as not found",
    );

    let missing = queries
        .get_readlist_detail(
            &context,
            ReadListDetailQuery {
                readlist_id: "readlist-missing".to_string(),
            },
        )
        .await
        .expect("missing readlist lookup should succeed");
    assert!(missing.is_none(), "missing readlist must return none");
}

pub(super) async fn phase6_readlist_detail_uses_existing_visibility_rules() {
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
        SeriesRow::new("series-lib2", "lib-2", "Library 2 Series")
            .with_labels(["safe"])
            .with_age_rating(12),
    );

    adapter.insert_book(BookRow::new("book-safe", "series-safe", "lib-1", "Book Safe"));
    adapter.insert_book(BookRow::new(
        "book-adult",
        "series-adult",
        "lib-1",
        "Book Adult",
    ));
    adapter.insert_book(BookRow::new(
        "book-lib2",
        "series-lib2",
        "lib-2",
        "Book Library 2",
    ));
    adapter.insert_read_list(
        ReadListRow::new("readlist-shared", "ReadList Shared")
            .with_summary("shared")
            .with_book_ids(["book-safe", "book-adult", "book-lib2"]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = restricted_context();

    let detail = queries
        .get_readlist_detail(
            &context,
            ReadListDetailQuery {
                readlist_id: "readlist-shared".to_string(),
            },
        )
        .await
        .expect("readlist detail query should succeed")
        .expect("partially visible readlist should remain accessible");

    let via_book_readlists = queries
        .list_book_readlists(
            &context,
            BookReadlistsQuery {
                book_id: "book-safe".to_string(),
            },
        )
        .await
        .expect("book readlists query should succeed");
    let from_existing_rules = via_book_readlists
        .into_iter()
        .find(|it| it.id == "readlist-shared")
        .expect("existing readlist visibility query should include readlist-shared");

    assert_eq!(detail.book_ids, from_existing_rules.book_ids);
    assert_eq!(detail.filtered, from_existing_rules.filtered);
}

pub(super) async fn phase6_readlist_detail_empty_accessible_readlist_remains_visible() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(
        SeriesRow::new("series-safe", "lib-1", "Safe Series")
            .with_labels(["safe"])
            .with_age_rating(12),
    );
    adapter.insert_book(BookRow::new("book-safe", "series-safe", "lib-1", "Book Safe"));
    adapter.insert_read_list(
        ReadListRow::new("readlist-empty", "ReadList Empty")
            .with_summary("empty")
            .with_book_ids::<[&str; 0], &str>([]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = restricted_context();

    let detail = queries
        .get_readlist_detail(
            &context,
            ReadListDetailQuery {
                readlist_id: "readlist-empty".to_string(),
            },
        )
        .await
        .expect("empty readlist detail lookup should succeed")
        .expect("empty readlist should still be visible");

    assert_eq!(detail.id, "readlist-empty");
    assert!(detail.book_ids.is_empty());
    assert!(!detail.filtered);
}
