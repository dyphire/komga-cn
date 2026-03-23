use super::{
    BooksListQuery, DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueries,
    DiscoveryQueryContext, LibraryListQuery, SeriesListQuery, SqliteDiscoveryAdapter,
};

const DISCOVERY_QUERY_SOURCE: &str =
    include_str!("../../crates/persistence/src/read_models/queries.rs");

#[tokio::test]
async fn unsupported_sorts_are_classified_non_native() {
    let use_cases = DiscoveryQueries::new(SqliteDiscoveryAdapter::default());
    let context = DiscoveryQueryContext::allow_all();

    let series = use_cases
        .list_series(
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
        )
        .await;
    let books = use_cases
        .list_books(
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
        )
        .await;

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
        .await
        .expect("libraries should not require sort classification");
}

#[test]
fn reference_slice_is_sqlx_only_after_discovery_finalization() {
    for fragment in [
        "fn list_libraries_sqlx(",
        "fn resolve_series_resource_sqlx(",
        "fn resolve_book_resource_sqlx(",
        "fn map_sqlx_error(",
        "#[derive(sqlx::FromRow)]",
    ] {
        assert!(
            DISCOVERY_QUERY_SOURCE.contains(fragment),
            "sqlx discovery slice should stay explicit: {fragment}",
        );
    }

    for removed_fragment in [
        "fn list_libraries_legacy(",
        "fn resolve_series_resource_legacy(",
        "fn resolve_book_resource_legacy(",
        "fn with_sqlx_snapshot<",
    ] {
        assert!(
            !DISCOVERY_QUERY_SOURCE.contains(removed_fragment),
            "legacy discovery fallback fragment should be removed: {removed_fragment}",
        );
    }
}
