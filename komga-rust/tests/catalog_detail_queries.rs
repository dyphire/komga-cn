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

mod compat;

const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-compat-search-ownership";
const NATIVE_OWNERSHIP_MARKER: &str = "native-rust-owned";
const USER_BASIC_AUTH: &str = "dXNlckBleGFtcGxlLm9yZzp1c2Vy";
const RESTRICTED_BASIC_AUTH: &str = "cmVzdHJpY3RlZEBleGFtcGxlLm9yZzpyZXN0cmljdGVk";

use compat::http::{page_content_ids, response_json, session_token_for_basic_auth};

#[path = "catalog_detail_queries/navigation.rs"]
mod navigation;
#[path = "catalog_detail_queries/oneshot_bootstrap.rs"]
mod oneshot_bootstrap;
#[path = "catalog_detail_queries/readlist_books.rs"]
mod readlist_books;
#[path = "catalog_detail_queries/visibility.rs"]
mod visibility;

fn restricted_context() -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: Some("user-1".to_string()),
        is_admin: false,
        authorized_library_ids: Some(vec!["lib-1".to_string()]),
        restrictions: Some(QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec![],
            labels_exclude: vec!["adult".to_string()],
        }),
    }
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
