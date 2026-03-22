use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use komga_rust::application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, BooksListQuery, DiscoveryQueries,
    ReadListBooksQuery, SeriesCollectionsQuery, SeriesDetailQuery,
};
use komga_rust::domain::discovery::{
    AgeRestrictionKind, DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueryContext,
    QueryRestrictions,
};
use komga_rust::persistence::discovery::{
    BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow,
    SqliteDiscoveryAdapter,
};
use serde_json::Value;
use tower::util::ServiceExt;

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

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

#[test]
fn readlist_books_follow_legacy_ordered_and_unordered_semantics() {
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
    let context = DiscoveryQueryContext::allow_all();

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

#[test]
fn readlist_books_cover_restricted_and_empty_fixtures() {
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
        .expect("fully filtered readlist books query should succeed");
    assert!(fully_filtered.content.is_empty());
}

#[tokio::test]
async fn oneshot_bootstrap_requires_visible_oneshot_series() {
    let app = komga_rust::app::build_router();
    let user_token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;
    let restricted_token = session_token_for_basic_auth(&app, RESTRICTED_BASIC_AUTH).await;

    let owned = post_books_list(
        &app,
        &user_token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
    )
    .await;
    assert_eq!(owned.status(), StatusCode::OK);
    assert_eq!(
        owned
            .headers()
            .get(SEARCH_OWNERSHIP_HEADER)
            .and_then(|v| v.to_str().ok()),
        Some(NATIVE_OWNERSHIP_MARKER),
    );
    let owned_json = response_json(owned).await;
    assert_eq!(page_content_ids(&owned_json), vec!["book-oneshot"]);
    assert!(owned_json.get("_compat").is_none());

    let hidden = post_books_list(
        &app,
        &restricted_token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot-restricted"}}"#,
    )
    .await;
    assert_eq!(hidden.status(), StatusCode::OK);
    let hidden_json = response_json(hidden).await;
    assert_eq!(
        hidden_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.visible-single-book)",
    );
}

#[tokio::test]
async fn oneshot_bootstrap_rejects_non_oneshot_and_wide_books_list_shapes() {
    let app = komga_rust::app::build_router();
    let token = session_token_for_basic_auth(&app, USER_BASIC_AUTH).await;

    let non_oneshot = post_books_list(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-1"}}"#,
    )
    .await;
    assert_eq!(non_oneshot.status(), StatusCode::OK);
    let non_oneshot_json = response_json(non_oneshot).await;
    assert_eq!(
        non_oneshot_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.series-not-oneshot)",
    );

    let multi_book = post_books_list(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot-multi"}}"#,
    )
    .await;
    assert_eq!(multi_book.status(), StatusCode::OK);
    let multi_book_json = response_json(multi_book).await;
    assert_eq!(
        multi_book_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.visible-single-book)",
    );

    let query_params = post_books_list(
        &app,
        &token,
        "/api/v1/books/list?page=0&size=20",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
    )
    .await;
    assert_eq!(query_params.status(), StatusCode::OK);
    let query_params_json = response_json(query_params).await;
    assert_eq!(
        query_params_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.query-params)",
    );

    let readlist_context = post_books_list(
        &app,
        &token,
        "/api/v1/books/list?context=READLIST&contextId=readlist-2",
        r#"{"condition":{"type":"SeriesId","operator":"is","value":"series-oneshot"}}"#,
    )
    .await;
    assert_eq!(readlist_context.status(), StatusCode::OK);
    let readlist_context_json = response_json(readlist_context).await;
    assert_eq!(
        readlist_context_json["_compat"]["shape"],
        "UnsupportedBookFilter(oneshot-bootstrap.query-params)",
    );

    let wide_filter = post_books_list(
        &app,
        &token,
        "/api/v1/books/list",
        r#"{"condition":{"type":"AllOfBook","conditions":[{"type":"SeriesId","operator":"is","value":"series-oneshot"}]}}"#,
    )
    .await;
    assert_eq!(wide_filter.status(), StatusCode::OK);
    let wide_filter_json = response_json(wide_filter).await;
    assert!(
        wide_filter_json["_compat"]["shape"]
            .as_str()
            .unwrap_or_default()
            .starts_with("UnsupportedBook"),
        "wide books-list shape should stay explicit non-native",
    );
}

async fn session_token_for_basic_auth<S>(app: &S, basic_auth: &str) -> String
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v2/users/me")
                .header(header::AUTHORIZATION, format!("Basic {basic_auth}"))
                .header("X-Auth-Token", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    response
        .headers()
        .get("x-auth-token")
        .expect("login response should include x-auth-token")
        .to_str()
        .expect("x-auth-token should be valid UTF-8")
        .to_string()
}

async fn post_books_list<S>(app: &S, token: &str, uri: &str, body: &str) -> axum::response::Response
where
    S: tower::Service<Request<Body>, Response = axum::response::Response> + Clone,
    S::Error: std::fmt::Debug,
    S::Future: Send,
{
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("X-Auth-Token", token)
                .header(SEARCH_OWNERSHIP_HEADER, NATIVE_OWNERSHIP_MARKER)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn response_json(response: axum::response::Response) -> Value {
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

fn page_content_ids(value: &Value) -> Vec<&str> {
    value["content"]
        .as_array()
        .expect("page payload should expose array content")
        .iter()
        .map(|it| {
            it.get("id")
                .and_then(Value::as_str)
                .expect("page item id should be a string")
        })
        .collect()
}
