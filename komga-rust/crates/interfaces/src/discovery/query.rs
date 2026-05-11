use axum::Json;
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    BooksBrowseRequest, LatestBooksRequest, PageRequest, SeriesAlphabeticalGroupsRequest,
    SeriesBrowseRequest,
};
use komga_domain::common_ids::{CollectionId, LibraryId};
use komga_domain::discovery::{
    AgeRatingCondition, CompositeSeriesCondition, DateCondition, DiscoveryError, FilterOperator,
    InclusionCondition, ReadStatusCondition, SeriesCondition, SeriesFilter, SeriesSort,
    SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
use serde_json::Value;

use super::books::list_query::{
    build_legacy_books_filter, legacy_series_books_book_filter,
    legacy_series_books_sort_from_query, normalize_release_date_date_time,
    parse_book_filter_from_json, parse_book_sorts_from_json, parse_book_sorts_from_json_values,
};
use super::persisted::common_helpers::{decode_query_component, requested_query_values};
use super::series::{
    parse_legacy_series_sorts, parse_series_filter_from_json, parse_series_sorts_from_json,
    parse_series_sorts_from_json_values,
};
use super::series_routes::author_query_to_author_match;
use crate::helpers::{extract_full_text_search, query_bool, query_value, query_values};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::discovery) struct BrowseResponseMetadata {
    pub paged: bool,
    pub sorted: bool,
    pub kotlin_unpaged_shape: bool,
    pub empty_page_on_unmapped_library: bool,
}

impl BrowseResponseMetadata {
    fn new(unpaged: bool, sorted: bool) -> Self {
        Self {
            paged: !unpaged,
            sorted,
            kotlin_unpaged_shape: false,
            empty_page_on_unmapped_library: false,
        }
    }

    fn empty_page_on_unmapped_library(mut self, value: bool) -> Self {
        self.empty_page_on_unmapped_library = value;
        self
    }
}

#[derive(Clone, Debug)]
pub(in crate::discovery) struct ResolvedBooksBrowseRequest {
    pub request: BooksBrowseRequest,
    pub response: BrowseResponseMetadata,
}

#[derive(Clone, Debug)]
pub(in crate::discovery) struct ResolvedSeriesBrowseRequest {
    pub request: SeriesBrowseRequest,
    pub response: BrowseResponseMetadata,
}

#[derive(Clone, Debug)]
pub(in crate::discovery) struct ResolvedLatestBooksRequest {
    pub request: LatestBooksRequest,
    pub response: BrowseResponseMetadata,
}

#[derive(Clone, Debug)]
pub(in crate::discovery) struct ResolvedSeriesAlphabeticalGroupsRequest {
    pub request: SeriesAlphabeticalGroupsRequest,
}

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

    pub fn into_response(self) -> Response {
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

impl From<DiscoveryError> for QueryResolveError {
    fn from(error: DiscoveryError) -> Self {
        Self::InvalidSemantics(format!("{error:?}"))
    }
}

pub(in crate::discovery) fn resolve_books_list_request(
    uri: &Uri,
    payload: Value,
) -> Result<ResolvedBooksBrowseRequest, QueryResolveError> {
    if !payload.is_object() {
        return Err(QueryResolveError::BadRequest);
    }

    let filter = parse_book_filter_from_json(payload.get("condition")).map_err(|error| {
        QueryResolveError::InvalidSemantics(format!("invalid book filter: {error:?}"))
    })?;
    let search = normalized_full_text_search(&payload);
    let has_search = search.is_some();
    let query = uri.query().unwrap_or_default();
    let query_sort_values = decoded_query_values(query, "sort");
    let sort = if query_sort_values.is_empty() {
        parse_book_sorts_from_json(payload.get("sort"), has_search)
    } else {
        parse_book_sorts_from_json_values(&query_sort_values, has_search)
    };
    let page = resolve_usize(query, &payload, "page", 0);
    let size = resolve_usize(query, &payload, "size", 20).max(1);
    let unpaged = resolve_bool(query, &payload, "unpaged", false);
    let sorted = !sort.is_empty();

    Ok(ResolvedBooksBrowseRequest {
        request: BooksBrowseRequest {
            filter,
            sort,
            search,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata::new(unpaged, sorted),
    })
}

pub(in crate::discovery) fn resolve_deprecated_books_request(
    uri: &Uri,
    library_ids: Option<Vec<String>>,
    empty_page_on_unmapped_library: bool,
) -> Result<ResolvedBooksBrowseRequest, QueryResolveError> {
    let query = uri.query().unwrap_or_default();
    let tags = requested_query_values(query, "tag");
    let read_statuses = requested_query_values(query, "read_status");
    let media_statuses = requested_query_values(query, "media_status").map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_lowercase())
            .collect()
    });
    let released_after = match query_value(query, "released_after") {
        Some(value) => {
            let decoded = decode_query_component(value);
            Some(normalize_release_date_date_time(&decoded).ok_or(QueryResolveError::BadRequest)?)
        }
        None => None,
    };
    let page = query_usize(query, "page", 0);
    let size = query_usize(query, "size", 20).max(1);
    let unpaged = query_bool(query, "unpaged");
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sorted = !query_values(query, "sort").is_empty() || search.is_some();

    Ok(ResolvedBooksBrowseRequest {
        request: BooksBrowseRequest {
            filter: build_legacy_books_filter(
                library_ids,
                tags,
                read_statuses,
                media_statuses,
                released_after,
            ),
            sort: vec![],
            search,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata::new(unpaged, sorted)
            .empty_page_on_unmapped_library(empty_page_on_unmapped_library),
    })
}

pub(in crate::discovery) fn resolve_series_books_request(
    series_id: &str,
    uri: &Uri,
) -> Result<ResolvedBooksBrowseRequest, QueryResolveError> {
    let filter = legacy_series_books_book_filter(series_id, uri)
        .map_err(|_| QueryResolveError::BadRequest)?;
    let sort = legacy_series_books_sort_from_query(uri);
    let query = uri.query().unwrap_or_default();
    let page = query_usize(query, "page", 0);
    let size = query_usize(query, "size", 20).max(1);
    let unpaged = query_bool(query, "unpaged");
    let sorted = !query_values(query, "sort").is_empty();

    Ok(ResolvedBooksBrowseRequest {
        request: BooksBrowseRequest {
            filter,
            sort,
            search: None,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata::new(unpaged, sorted),
    })
}

pub(in crate::discovery) fn resolve_latest_books_request(
    uri: &Uri,
    library_ids: Option<Vec<String>>,
) -> ResolvedLatestBooksRequest {
    let query = uri.query().unwrap_or_default();
    let page = query_usize(query, "page", 0);
    let size = query_usize(query, "size", 20).max(1);
    let unpaged = query_bool(query, "unpaged");

    ResolvedLatestBooksRequest {
        request: LatestBooksRequest {
            library_ids,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata {
            paged: true,
            sorted: true,
            kotlin_unpaged_shape: unpaged,
            empty_page_on_unmapped_library: false,
        },
    }
}

pub(in crate::discovery) fn resolve_series_list_request(
    uri: &Uri,
    payload: Value,
) -> Result<ResolvedSeriesBrowseRequest, QueryResolveError> {
    if !payload.is_object() {
        return Err(QueryResolveError::BadRequest);
    }

    let filter = parse_series_filter_from_json(payload.get("condition")).map_err(|error| {
        QueryResolveError::InvalidSemantics(format!("invalid series filter: {error:?}"))
    })?;
    let search = normalized_full_text_search(&payload);
    let has_search = search.is_some();
    let query = uri.query().unwrap_or_default();
    let query_sort_values = decoded_query_values(query, "sort");
    let sort = if query_sort_values.is_empty() {
        parse_series_sorts_from_json(payload.get("sort"), has_search)
    } else {
        parse_series_sorts_from_json_values(&query_sort_values, has_search)
    };
    let page = resolve_usize(query, &payload, "page", 0);
    let size = resolve_usize(query, &payload, "size", 20).max(1);
    let unpaged = resolve_bool(query, &payload, "unpaged", false);
    let sorted = !sort.is_empty();

    Ok(ResolvedSeriesBrowseRequest {
        request: SeriesBrowseRequest {
            filter,
            sort,
            search,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata::new(unpaged, sorted),
    })
}

pub(in crate::discovery) fn resolve_deprecated_series_request(
    uri: &Uri,
) -> Result<ResolvedSeriesBrowseRequest, QueryResolveError> {
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let collection_ids = decoded_query_values_option(query, "collection_id");
    let collection_ids_for_sort = collection_ids.clone();
    let metadata_status = decoded_query_values_option(query, "status");
    let read_status = decoded_query_values_option(query, "read_status");
    let publishers = decoded_query_values_option(query, "publisher");
    let languages = decoded_query_values_option(query, "language");
    let genres = decoded_query_values_option(query, "genre");
    let tags = decoded_query_values_option(query, "tag");
    let age_ratings = decoded_query_values_option(query, "age_rating");
    let release_years = decoded_query_values_option(query, "release_year");
    let sharing_labels = decoded_query_values_option(query, "sharing_label");
    let authors = decoded_query_values_option(query, "author");
    let page = query_usize(query, "page", 0);
    let size = query_usize(query, "size", 20).max(1);
    let unpaged = query_bool(query, "unpaged");
    let deleted = optional_query_bool(query, "deleted")?;
    let oneshot = optional_query_bool(query, "oneshot")?;
    let complete = optional_query_bool(query, "complete")?;
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sort_values = decoded_query_values(query, "sort");

    let mut conditions = Vec::new();
    if let Some(ids) = &requested_library_ids
        && !ids.is_empty()
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::LibraryId(
            InclusionCondition::Include(ids.iter().cloned().map(LibraryId::from).collect()),
        )));
    }
    if let Some(ids) = collection_ids.filter(|ids| !ids.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::CollectionId(
            InclusionCondition::Include(ids.into_iter().map(CollectionId::from).collect()),
        )));
    }
    if let Some(value) = deleted {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Deleted(value)));
    }
    if let Some(value) = oneshot {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::OneShot(value)));
    }
    if let Some(value) = complete {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Complete(
            value,
        )));
    }
    if let Some(statuses) = metadata_status.filter(|values| !values.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::SeriesStatus(
            SeriesStatusCondition::Include(statuses),
        )));
    }
    if let Some(statuses) = read_status.filter(|values| !values.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::ReadStatus(
            ReadStatusCondition::Include(statuses),
        )));
    }
    if let Some(values) = publishers.filter(|values| !values.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Publisher(
            InclusionCondition::Include(values),
        )));
    }
    if let Some(values) = languages.filter(|values| !values.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Language(
            InclusionCondition::Include(values),
        )));
    }
    if let Some(values) = lowercase_values(genres) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Genre(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }
    if let Some(values) = lowercase_values(tags) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Tag(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }
    if let Some(values) = age_ratings.filter(|values| !values.is_empty()) {
        let mut ratings = Vec::new();
        let mut include_empty = false;
        for value in values {
            match value.parse::<u16>() {
                Ok(rating) => ratings.push(rating),
                Err(_) => include_empty = true,
            }
        }
        if include_empty {
            conditions.push(SeriesCondition::Value(SeriesValueCondition::AgeRating(
                AgeRatingCondition::ExactOrEmpty(ratings),
            )));
        } else if !ratings.is_empty() {
            conditions.push(SeriesCondition::Value(SeriesValueCondition::AgeRating(
                AgeRatingCondition::Exact(InclusionCondition::Include(ratings)),
            )));
        }
    }
    if let Some(values) = release_years
        .map(|values| {
            values
                .into_iter()
                .filter_map(|value| value.parse::<i32>().ok().map(|year| year.to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::ReleaseDate(
            DateCondition::StartsWith(InclusionCondition::Include(values)),
        )));
    }
    if let Some(values) = lowercase_values(sharing_labels) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::SharingLabel(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }
    if let Some(values) = authors
        .map(|values| {
            values
                .into_iter()
                .filter_map(author_query_to_filter_token)
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Author(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }

    let filter = SeriesFilter {
        condition: match conditions.len() {
            0 => None,
            1 => conditions.pop(),
            _ => Some(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::All,
                conditions,
            })),
        },
    };
    let sort = parse_legacy_series_sorts(
        &sort_values,
        search.as_deref(),
        collection_ids_for_sort.as_ref(),
    );
    let sorted = !sort.is_empty();

    Ok(ResolvedSeriesBrowseRequest {
        request: SeriesBrowseRequest {
            filter,
            sort,
            search,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata::new(unpaged, sorted),
    })
}

pub(in crate::discovery) fn resolve_series_feed_request(
    uri: &Uri,
    sort: Vec<SeriesSort>,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Result<ResolvedSeriesBrowseRequest, QueryResolveError> {
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let page = query_usize(query, "page", 0);
    let size = query_usize(query, "size", 20).max(1);
    let unpaged = query_bool(query, "unpaged");
    let deleted = optional_query_bool(query, "deleted")?;
    let oneshot = optional_query_bool(query, "oneshot")?;

    let mut conditions = Vec::new();
    if let Some(ids) = &requested_library_ids
        && !ids.is_empty()
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::LibraryId(
            InclusionCondition::Include(ids.iter().cloned().map(LibraryId::from).collect()),
        )));
    }
    if let Some(value) = deleted {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Deleted(value)));
    }
    if let Some(value) = oneshot {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::OneShot(value)));
    }
    if exclude_newly_added {
        conditions.push(SeriesCondition::Value(
            SeriesValueCondition::ExcludeNewlyAdded(true),
        ));
    }

    let filter = SeriesFilter {
        condition: match conditions.len() {
            0 => None,
            1 => conditions.pop(),
            _ => Some(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::All,
                conditions,
            })),
        },
    };
    let paged = if unpaged && kotlin_unpaged_page_shape {
        true
    } else {
        !unpaged
    };

    Ok(ResolvedSeriesBrowseRequest {
        request: SeriesBrowseRequest {
            filter,
            sort,
            search: None,
            page: PageRequest {
                page,
                size,
                unpaged,
            },
        },
        response: BrowseResponseMetadata {
            paged,
            sorted: true,
            kotlin_unpaged_shape: unpaged && kotlin_unpaged_page_shape,
            empty_page_on_unmapped_library: false,
        },
    })
}

pub(in crate::discovery) fn resolve_series_alphabetical_groups_request(
    body: Value,
) -> Result<ResolvedSeriesAlphabeticalGroupsRequest, QueryResolveError> {
    if !body.is_object() {
        return Err(QueryResolveError::BadRequest);
    }

    let filter = parse_series_filter_from_json(body.get("condition")).map_err(|error| {
        QueryResolveError::InvalidSemantics(format!(
            "invalid series alphabetical-groups request: {error:?}",
        ))
    })?;

    Ok(ResolvedSeriesAlphabeticalGroupsRequest {
        request: SeriesAlphabeticalGroupsRequest {
            filter,
            search: normalized_full_text_search(&body),
        },
    })
}

fn query_usize(query: &str, key: &str, default: usize) -> usize {
    query_value(query, key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

fn resolve_usize(query: &str, payload: &Value, key: &str, default: usize) -> usize {
    query_value(query, key)
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            payload
                .get(key)
                .and_then(Value::as_u64)
                .map(|value| value as usize)
        })
        .unwrap_or(default)
}

fn resolve_bool(query: &str, payload: &Value, key: &str, default: bool) -> bool {
    query_value(query, key)
        .map(|_| query_bool(query, key))
        .or_else(|| payload.get(key).and_then(Value::as_bool))
        .unwrap_or(default)
}

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, QueryResolveError> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(QueryResolveError::BadRequest),
        None => Ok(None),
    }
}

fn decoded_query_values(query: &str, key: &str) -> Vec<String> {
    query_values(query, key)
        .into_iter()
        .map(decode_query_component)
        .collect()
}

fn decoded_query_values_option(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(|value| decode_query_component(value.trim()))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

fn normalized_full_text_search(payload: &Value) -> Option<String> {
    extract_full_text_search(payload).filter(|value| !value.trim().is_empty())
}

fn lowercase_values(values: Option<Vec<String>>) -> Option<Vec<String>> {
    values
        .map(|values| {
            values
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .filter(|values| !values.is_empty())
}

fn author_query_to_filter_token(value: String) -> Option<String> {
    let encoded = author_query_to_author_match(value);
    let object = encoded.as_object()?;
    if object.is_empty() {
        return None;
    }
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    match (name, role) {
        (Some(name), Some(role)) => Some(format!("{name}::{role}")),
        _ => Some(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use axum::http::Uri;
    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::{
        BookCondition, BookSort, BookValueCondition, InclusionCondition, SeriesSort,
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
}
