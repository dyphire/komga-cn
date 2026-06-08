mod books;
mod collections;
mod readlists;
mod series;

use super::{
    BooksBrowseRequest, LatestBooksRequest, PageRequest, SeriesAlphabeticalGroupsRequest,
    SeriesBrowseRequest,
};
use books::{
    build_legacy_books_filter, legacy_series_books_book_filter,
    legacy_series_books_sort_from_query, normalize_release_date_date_time,
    parse_book_filter_from_json, parse_book_sorts_from_json, parse_book_sorts_from_json_values,
};
pub use collections::{ResolvedCollectionListRequest, resolve_collection_list_request};
use komga_domain::common_ids::{CollectionId, LibraryId};
use komga_domain::discovery::{
    AgeRatingCondition, CompositeSeriesCondition, DateCondition, DiscoveryError, FilterOperator,
    InclusionCondition, ReadStatusCondition, SeriesCondition, SeriesFilter, SeriesSort,
    SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
pub use readlists::{
    normalize_readlists_search, parse_readlists_sort, resolve_readlist_books_query,
    resolve_readlists_query,
};
use serde_json::Value;
use series::{
    parse_legacy_series_sorts, parse_series_sorts_from_json, parse_series_sorts_from_json_values,
};

pub use series::parse_series_filter_from_json;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BrowseResponseMetadata {
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
pub struct ResolvedBooksBrowseRequest {
    pub request: BooksBrowseRequest,
    pub response: BrowseResponseMetadata,
}

#[derive(Clone, Debug)]
pub struct ResolvedSeriesBrowseRequest {
    pub request: SeriesBrowseRequest,
    pub response: BrowseResponseMetadata,
}

#[derive(Clone, Debug)]
pub struct ResolvedLatestBooksRequest {
    pub request: LatestBooksRequest,
    pub response: BrowseResponseMetadata,
}

#[derive(Clone, Debug)]
pub struct ResolvedSeriesAlphabeticalGroupsRequest {
    pub request: SeriesAlphabeticalGroupsRequest,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryRequestError {
    BadRequest,
    InvalidSemantics(String),
}

impl From<DiscoveryError> for DiscoveryRequestError {
    fn from(error: DiscoveryError) -> Self {
        Self::InvalidSemantics(format!("{error:?}"))
    }
}

pub fn resolve_books_list_request(
    query: &str,
    payload: Value,
) -> Result<ResolvedBooksBrowseRequest, DiscoveryRequestError> {
    if !payload.is_object() {
        return Err(DiscoveryRequestError::BadRequest);
    }

    let filter = parse_book_filter_from_json(payload.get("condition")).map_err(|error| {
        DiscoveryRequestError::InvalidSemantics(format!("invalid book filter: {error:?}"))
    })?;
    let search = normalized_full_text_search(&payload);
    let has_search = search.is_some();
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

pub fn resolve_deprecated_books_request(
    query: &str,
    library_ids: Option<Vec<String>>,
    empty_page_on_unmapped_library: bool,
) -> Result<ResolvedBooksBrowseRequest, DiscoveryRequestError> {
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
            Some(
                normalize_release_date_date_time(&decoded)
                    .ok_or(DiscoveryRequestError::BadRequest)?,
            )
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

pub fn resolve_series_books_request(
    series_id: &str,
    query: &str,
) -> Result<ResolvedBooksBrowseRequest, DiscoveryRequestError> {
    let filter = legacy_series_books_book_filter(series_id, query)?;
    let sort = legacy_series_books_sort_from_query(query);
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

pub fn resolve_latest_books_request(
    query: &str,
    library_ids: Option<Vec<String>>,
) -> ResolvedLatestBooksRequest {
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

pub fn resolve_series_list_request(
    query: &str,
    payload: Value,
) -> Result<ResolvedSeriesBrowseRequest, DiscoveryRequestError> {
    if !payload.is_object() {
        return Err(DiscoveryRequestError::BadRequest);
    }

    let filter = parse_series_filter_from_json(payload.get("condition")).map_err(|error| {
        DiscoveryRequestError::InvalidSemantics(format!("invalid series filter: {error:?}"))
    })?;
    let search = normalized_full_text_search(&payload);
    let has_search = search.is_some();
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

pub fn resolve_deprecated_series_request(
    query: &str,
) -> Result<ResolvedSeriesBrowseRequest, DiscoveryRequestError> {
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

pub fn resolve_series_feed_request(
    query: &str,
    sort: Vec<SeriesSort>,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Result<ResolvedSeriesBrowseRequest, DiscoveryRequestError> {
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

pub fn resolve_series_alphabetical_groups_request(
    body: Value,
) -> Result<ResolvedSeriesAlphabeticalGroupsRequest, DiscoveryRequestError> {
    if !body.is_object() {
        return Err(DiscoveryRequestError::BadRequest);
    }

    let filter = parse_series_filter_from_json(body.get("condition")).map_err(|error| {
        DiscoveryRequestError::InvalidSemantics(format!(
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

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, DiscoveryRequestError> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(DiscoveryRequestError::BadRequest),
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

fn requested_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

fn normalized_full_text_search(payload: &Value) -> Option<String> {
    payload
        .get("fullTextSearch")
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
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
    let decoded = decode_query_component(&value);
    let (name, role) = decoded
        .split_once(',')
        .map(|(name, role)| (name.trim(), role.trim()))?;
    let name = (!name.is_empty()).then(|| name.to_ascii_lowercase());
    let role = (!role.is_empty()).then(|| role.to_ascii_lowercase());

    match (name, role) {
        (Some(name), Some(role)) => Some(format!("{name}::{role}")),
        _ => Some(String::new()),
    }
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_values<'a>(query: &'a str, key: &str) -> Vec<&'a str> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            Some(parts.next().unwrap_or_default())
        })
        .collect()
}

fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

fn decode_query_component(value: &str) -> String {
    let mut decoded = Vec::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                let first = (bytes[index + 1] as char).to_digit(16);
                let second = (bytes[index + 2] as char).to_digit(16);

                if let (Some(first), Some(second)) = (first, second) {
                    decoded.push((first * 16 + second) as u8);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }

    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use komga_domain::common_ids::LibraryId;
    use komga_domain::discovery::{
        BookCondition, BookSort, BookValueCondition, FilterOperator, InclusionCondition,
        SeriesCondition, SeriesSort, SeriesValueCondition, StringCondition,
    };
    use serde_json::json;

    #[test]
    fn books_list_query_values_override_json_body_inside_application_boundary() {
        let body = json!({
            "page": 1,
            "size": 2,
            "unpaged": false,
            "sort": ["metadata.title,asc"]
        });

        let resolved =
            resolve_books_list_request("page=3&size=7&unpaged=true&sort=name,desc", body)
                .expect("books list request should resolve");

        assert_eq!(resolved.request.page.page, 3);
        assert_eq!(resolved.request.page.size, 7);
        assert!(resolved.request.page.unpaged);
        assert_eq!(resolved.request.sort, vec![BookSort::NameDesc]);
        assert!(resolved.response.sorted);
        assert!(!resolved.response.paged);
    }

    #[test]
    fn books_list_defaults_to_relevance_sort_for_non_blank_search() {
        let resolved = resolve_books_list_request("", json!({ "fullTextSearch": "robot" }))
            .expect("books list request should resolve");

        assert_eq!(resolved.request.search.as_deref(), Some("robot"));
        assert_eq!(resolved.request.sort, vec![BookSort::RelevanceAsc]);
        assert!(resolved.response.sorted);
    }

    #[test]
    fn deprecated_books_request_maps_library_ids_into_domain_filter() {
        let resolved = resolve_deprecated_books_request(
            "library_id=legacy-library",
            Some(vec!["mapped-library".to_string()]),
            false,
        )
        .expect("deprecated books request should resolve");

        assert_eq!(
            resolved.request.filter.condition,
            Some(BookCondition::Value(BookValueCondition::LibraryId(
                InclusionCondition::Include(vec![LibraryId::from("mapped-library")])
            )))
        );
    }

    #[test]
    fn legacy_keyed_series_filter_parses_inside_application_boundary() {
        let filter = parse_series_filter_from_json(Some(&json!({
            "anyOf": [
                {"title": {"operator": "contains", "value": "Saga"}},
                {"tag": {"operator": "is", "value": "Favorite"}}
            ]
        })))
        .expect("legacy keyed series filter should parse");

        let Some(SeriesCondition::Composite(condition)) = filter.condition else {
            panic!("legacy keyed anyOf should produce a composite condition");
        };
        assert_eq!(condition.operator, FilterOperator::Any);
        assert_eq!(condition.conditions.len(), 2);
    }

    #[test]
    fn deprecated_series_rejects_invalid_bool_inside_application_boundary() {
        assert!(matches!(
            resolve_deprecated_series_request("deleted=maybe"),
            Err(DiscoveryRequestError::BadRequest),
        ));
    }

    #[test]
    fn deprecated_series_author_with_missing_name_or_role_keeps_legacy_empty_token() {
        let resolved = resolve_deprecated_series_request("author=Jane+Doe%2C")
            .expect("deprecated series request should resolve");

        assert_eq!(
            resolved.request.filter.condition,
            Some(SeriesCondition::Value(SeriesValueCondition::Author(
                StringCondition::Exact(InclusionCondition::Include(vec!["".to_string()]))
            )))
        );
    }

    #[test]
    fn series_list_query_sort_overrides_json_sort() {
        let resolved = resolve_series_list_request(
            "sort=name,desc",
            json!({ "sort": ["metadata.titleSort,asc"] }),
        )
        .expect("series list request should resolve");

        assert_eq!(resolved.request.sort, vec![SeriesSort::NameDesc]);
        assert!(resolved.response.sorted);
    }
}
