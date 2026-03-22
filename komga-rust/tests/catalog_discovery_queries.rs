use komga_rust::application::discovery::{
    BooksListQuery, DiscoveryQueries, LibraryListQuery, SeriesListQuery,
};
use komga_rust::domain::discovery::{
    AgeRestrictionKind, DirectBrowseBooksListFamily, DiscoveryError, DiscoveryQueryContext,
    QueryRestrictions,
};
use komga_rust::persistence::discovery::{BookRow, LibraryRow, SeriesRow, SqliteDiscoveryAdapter};

#[path = "catalog_discovery_queries/authorization.rs"]
mod authorization;
#[path = "catalog_discovery_queries/extended_filters.rs"]
mod extended_filters;
#[path = "catalog_discovery_queries/request_shape.rs"]
mod request_shape;
#[path = "catalog_discovery_queries/sorting.rs"]
mod sorting;

fn restricted_context() -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["nsfw".to_string()],
        }),
    }
}

fn restricted_library_series_adapter() -> SqliteDiscoveryAdapter {
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
    adapter
}
