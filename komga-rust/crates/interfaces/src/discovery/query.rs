use axum::Json;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{ReadListBooksQuery, ReadListsQuery};
use komga_domain::discovery::SeriesSort;
use serde_json::Value;

use super::request_resolution::{
    DiscoveryRequestError, ResolvedBooksBrowseRequest, ResolvedCollectionListRequest,
    ResolvedLatestBooksRequest, ResolvedSeriesAlphabeticalGroupsRequest,
    ResolvedSeriesBrowseRequest,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::discovery) enum QueryResolveError {
    BadRequest,
    InvalidSemantics(String),
}

impl QueryResolveError {
    #[cfg(test)]
    pub fn status(&self) -> StatusCode {
        match self {
            Self::BadRequest | Self::InvalidSemantics(_) => StatusCode::BAD_REQUEST,
        }
    }

    pub(super) fn into_response(self) -> Response {
        match self {
            Self::BadRequest => StatusCode::BAD_REQUEST.into_response(),
            Self::InvalidSemantics(error) => (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": error })),
            )
                .into_response(),
        }
    }
}

impl From<DiscoveryRequestError> for QueryResolveError {
    fn from(error: DiscoveryRequestError) -> Self {
        match error {
            DiscoveryRequestError::BadRequest => Self::BadRequest,
            DiscoveryRequestError::InvalidSemantics(error) => Self::InvalidSemantics(error),
        }
    }
}

pub(in crate::discovery) fn resolve_books_list_request(
    uri: &Uri,
    payload: Value,
) -> Result<ResolvedBooksBrowseRequest, QueryResolveError> {
    super::request_resolution::resolve_books_list_request(query(uri), payload)
        .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_deprecated_books_request(
    uri: &Uri,
    library_ids: Option<Vec<String>>,
    empty_page_on_unmapped_library: bool,
) -> Result<ResolvedBooksBrowseRequest, QueryResolveError> {
    super::request_resolution::resolve_deprecated_books_request(
        query(uri),
        library_ids,
        empty_page_on_unmapped_library,
    )
    .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_series_books_request(
    series_id: &str,
    uri: &Uri,
) -> Result<ResolvedBooksBrowseRequest, QueryResolveError> {
    super::request_resolution::resolve_series_books_request(series_id, query(uri))
        .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_latest_books_request(
    uri: &Uri,
    library_ids: Option<Vec<String>>,
) -> ResolvedLatestBooksRequest {
    super::request_resolution::resolve_latest_books_request(query(uri), library_ids)
}

pub(in crate::discovery) fn resolve_series_list_request(
    uri: &Uri,
    payload: Value,
) -> Result<ResolvedSeriesBrowseRequest, QueryResolveError> {
    super::request_resolution::resolve_series_list_request(query(uri), payload)
        .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_deprecated_series_request(
    uri: &Uri,
) -> Result<ResolvedSeriesBrowseRequest, QueryResolveError> {
    super::request_resolution::resolve_deprecated_series_request(query(uri))
        .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_series_feed_request(
    uri: &Uri,
    sort: Vec<SeriesSort>,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Result<ResolvedSeriesBrowseRequest, QueryResolveError> {
    super::request_resolution::resolve_series_feed_request(
        query(uri),
        sort,
        exclude_newly_added,
        kotlin_unpaged_page_shape,
    )
    .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_series_alphabetical_groups_request(
    body: Value,
) -> Result<ResolvedSeriesAlphabeticalGroupsRequest, QueryResolveError> {
    super::request_resolution::resolve_series_alphabetical_groups_request(body)
        .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_readlists_query(uri: &Uri) -> ReadListsQuery {
    super::request_resolution::resolve_readlists_query(query(uri))
}

pub(in crate::discovery) fn resolve_readlist_books_query(
    readlist_id: impl Into<String>,
    uri: &Uri,
) -> Result<ReadListBooksQuery, QueryResolveError> {
    super::request_resolution::resolve_readlist_books_query(readlist_id, query(uri))
        .map_err(DiscoveryRequestError::from)
        .map_err(QueryResolveError::from)
}

pub(in crate::discovery) fn resolve_collection_list_request(
    uri: &Uri,
) -> ResolvedCollectionListRequest {
    super::request_resolution::resolve_collection_list_request(query(uri))
}

pub(in crate::discovery) fn parse_series_filter_from_json(
    condition: Option<&Value>,
) -> Result<komga_domain::discovery::SeriesFilter, komga_domain::discovery::DiscoveryError> {
    super::request_resolution::parse_series_filter_from_json(condition)
}

fn query(uri: &Uri) -> &str {
    uri.query().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;
    use komga_application::discovery::ReadListsSort;
    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::{
        BookCondition, BookSort, BookValueCondition, InclusionCondition, MediaStatus, SeriesSort,
    };
    use serde_json::json;

    #[test]
    fn books_list_uri_pagination_sort_and_unpaged_override_json_body() {
        let uri: Uri = "/api/v1/books/list?page=3&size=7&unpaged=true&sort=name,desc"
            .parse()
            .unwrap();
        let body = json!({
            "page": 1,
            "size": 2,
            "unpaged": false,
            "sort": ["metadata.title,asc"]
        });

        let resolved = super::resolve_books_list_request(&uri, body).unwrap();

        assert_eq!(resolved.request.page.page, 3);
        assert_eq!(resolved.request.page.size, 7);
        assert!(resolved.request.page.unpaged);
        assert_eq!(resolved.request.sort, vec![BookSort::NameDesc]);
        assert!(resolved.response.sorted);
        assert!(!resolved.response.paged);
    }

    #[test]
    fn books_list_defaults_to_relevance_sort_for_non_blank_search_without_explicit_sort() {
        let uri: Uri = "/api/v1/books/list".parse().unwrap();
        let body = json!({ "fullTextSearch": "robot" });

        let resolved = super::resolve_books_list_request(&uri, body).unwrap();

        assert_eq!(resolved.request.search.as_deref(), Some("robot"));
        assert_eq!(resolved.request.sort, vec![BookSort::RelevanceAsc]);
        assert!(resolved.response.sorted);
    }

    #[test]
    fn books_list_rejects_invalid_media_status_inside_interfaces_boundary() {
        let uri: Uri = "/api/v1/books/list".parse().unwrap();
        let body = json!({
            "condition": {
                "type": "MediaStatus",
                "operator": "is",
                "value": "BROKEN_STATUS"
            }
        });

        let error = super::resolve_books_list_request(&uri, body).unwrap_err();

        assert_eq!(error.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn books_list_keeps_blank_search_unsorted() {
        let uri: Uri = "/api/v1/books/list".parse().unwrap();
        let body = json!({ "fullTextSearch": "   " });

        let resolved = super::resolve_books_list_request(&uri, body).unwrap();

        assert_eq!(resolved.request.search, None);
        assert!(resolved.request.sort.is_empty());
        assert!(!resolved.response.sorted);
    }

    #[test]
    fn deprecated_books_request_marks_unmapped_requested_library_as_empty_page() {
        let uri: Uri = "/api/v1/books?library_id=legacy-library&page=2&size=9"
            .parse()
            .unwrap();

        let resolved = super::resolve_deprecated_books_request(&uri, None, true).unwrap();

        assert!(resolved.response.empty_page_on_unmapped_library);
        assert_eq!(resolved.request.page.page, 2);
        assert_eq!(resolved.request.page.size, 9);
    }

    #[test]
    fn deprecated_books_request_maps_library_ids_into_domain_filter() {
        let uri: Uri = "/api/v1/books?library_id=legacy-library".parse().unwrap();

        let resolved = super::resolve_deprecated_books_request(
            &uri,
            Some(vec!["mapped-library".to_string()]),
            false,
        )
        .unwrap();

        let condition = resolved.request.filter.condition.unwrap();
        assert_eq!(
            condition,
            BookCondition::Value(BookValueCondition::LibraryId(InclusionCondition::Include(
                vec![LibraryId::from("mapped-library")]
            )))
        );
    }

    #[test]
    fn series_list_uri_sort_overrides_json_sort() {
        let uri: Uri = "/api/v1/series/list?sort=name,desc".parse().unwrap();
        let body = json!({ "sort": ["metadata.titleSort,asc"] });

        let resolved = super::resolve_series_list_request(&uri, body).unwrap();

        assert_eq!(resolved.request.sort, vec![SeriesSort::NameDesc]);
        assert!(resolved.response.sorted);
    }

    #[test]
    fn invalid_bool_in_legacy_series_request_is_bad_request() {
        let uri: Uri = "/api/v1/series?deleted=maybe".parse().unwrap();

        let error = super::resolve_deprecated_series_request(&uri).unwrap_err();

        assert_eq!(error.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn readlists_query_parsing_lives_at_the_interfaces_boundary() {
        let uri: Uri = "/api/v1/readlists?search=space&search=opera&sort=name,desc"
            .parse()
            .unwrap();

        let query = super::resolve_readlists_query(&uri);

        assert_eq!(query.search.as_deref(), Some("space,opera"));
        assert_eq!(query.sort, ReadListsSort::NameDesc);
    }

    #[test]
    fn readlist_books_query_parses_media_statuses_inside_interfaces_boundary() {
        let uri: Uri =
            "/api/v1/readlists/readlist-1/books?media_status=ready&media_status=OUTDATED"
                .parse()
                .unwrap();

        let query = super::resolve_readlist_books_query("readlist-1", &uri).unwrap();

        assert_eq!(
            query.media_statuses,
            Some(vec![MediaStatus::Ready, MediaStatus::Outdated])
        );
    }
}
