use super::{
    BookRow, DiscoveryQueries, LibraryRow, ReadListBooksQuery, ReadListRow, SeriesRow,
    SqliteDiscoveryAdapter, restricted_context,
};

#[tokio::test]
async fn readlist_books_follow_legacy_ordered_and_unordered_semantics() {
    let mut adapter = SqliteDiscoveryAdapter::default();
    adapter.insert_library(LibraryRow::new("lib-1", "Library 1"));
    adapter.insert_series(SeriesRow::new("series-1", "lib-1", "Series 1").with_labels(["safe"]));

    adapter.insert_book(
        BookRow::new("book-a", "series-1", "lib-1", "Book A")
            .with_number_sort(10)
            .with_release_date("2024-01-03"),
    );
    adapter.insert_book(
        BookRow::new("book-b", "series-1", "lib-1", "Book B")
            .with_number_sort(20)
            .with_release_date("2024-01-01"),
    );
    adapter.insert_book(BookRow::new("book-c", "series-1", "lib-1", "Book C").with_number_sort(30));

    adapter.insert_read_list(
        ReadListRow::new("readlist-ordered", "ReadList Ordered")
            .with_ordered(true)
            .with_book_ids(["book-a", "book-b", "book-c"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-unordered", "ReadList Unordered")
            .with_ordered(false)
            .with_book_ids(["book-a", "book-b", "book-c"]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let context = super::DiscoveryQueryContext::allow_all();

    let ordered = queries
        .list_readlist_books(
            &context,
            ReadListBooksQuery {
                readlist_id: "readlist-ordered".to_string(),
                page: 0,
                size: 20,
                unpaged: true,
                library_ids: None,
            },
        )
        .await
        .expect("ordered readlist books query should succeed");
    assert_eq!(
        ordered
            .content
            .iter()
            .map(|it| it.id.as_str())
            .collect::<Vec<_>>(),
        vec!["book-a", "book-b", "book-c"],
    );

    let unordered = queries
        .list_readlist_books(
            &context,
            ReadListBooksQuery {
                readlist_id: "readlist-unordered".to_string(),
                page: 0,
                size: 20,
                unpaged: true,
                library_ids: None,
            },
        )
        .await
        .expect("unordered readlist books query should succeed");
    assert_eq!(
        unordered
            .content
            .iter()
            .map(|it| it.id.as_str())
            .collect::<Vec<_>>(),
        vec!["book-c", "book-b", "book-a"],
    );
}

#[tokio::test]
async fn readlist_books_cover_restricted_and_empty_fixtures() {
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
        ReadListRow::new("readlist-mixed", "ReadList Mixed")
            .with_ordered(true)
            .with_book_ids(["book-safe", "book-adult"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-empty", "ReadList Empty")
            .with_ordered(true)
            .with_book_ids::<[&str; 0], &str>([]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-fully-filtered", "ReadList Fully Filtered")
            .with_ordered(true)
            .with_book_ids(["book-adult"]),
    );

    let queries = DiscoveryQueries::new(adapter);
    let restricted_context = restricted_context();

    let mixed = queries
        .list_readlist_books(
            &restricted_context,
            ReadListBooksQuery {
                readlist_id: "readlist-mixed".to_string(),
                page: 0,
                size: 20,
                unpaged: true,
                library_ids: None,
            },
        )
        .await
        .expect("mixed readlist books query should succeed");
    assert_eq!(
        mixed
            .content
            .iter()
            .map(|it| it.id.as_str())
            .collect::<Vec<_>>(),
        vec!["book-safe"],
    );

    let empty = queries
        .list_readlist_books(
            &restricted_context,
            ReadListBooksQuery {
                readlist_id: "readlist-empty".to_string(),
                page: 0,
                size: 20,
                unpaged: true,
                library_ids: None,
            },
        )
        .await
        .expect("empty readlist books query should succeed");
    assert!(empty.content.is_empty());

    let fully_filtered = queries
        .list_readlist_books(
            &restricted_context,
            ReadListBooksQuery {
                readlist_id: "readlist-fully-filtered".to_string(),
                page: 0,
                size: 20,
                unpaged: true,
                library_ids: None,
            },
        )
        .await
        .expect("fully filtered readlist books query should succeed");
    assert!(fully_filtered.content.is_empty());
}
