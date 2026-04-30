use super::filters::{OperatorValidationMode, parse_runtime_series_filters_with_mode};
use super::persisted::common_helpers::decode_query_component;
use super::persisted::delegates::{
    internal_error_response, invalid_runtime_series_list_response,
    load_persisted_alphabetical_groups, load_persisted_series_page,
    remap_requested_library_ids_for_persisted, requested_query_values,
    runtime_owned_series_list_response, series_page_payload,
};
use super::persisted::models::{
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, PersistedSeriesSummary,
    SeriesFilterCriteria,
};
use super::persisted::series_queries::parse_persisted_series_sort_modes;
use super::series_routes::author_query_to_author_match;
use super::*;
use axum::extract::State;
use komga_domain::discovery::PageEnvelope;
use std::sync::Arc;

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, ()> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(()),
        None => Ok(None),
    }
}

fn decoded_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(|value| decode_query_component(value.trim()))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

fn decoded_delimited_pair(query: &str, key: &str) -> Option<(String, String)> {
    let value = query_value(query, key)?;
    let value = decode_query_component(value);
    if value.trim().is_empty() {
        return None;
    }

    value
        .rsplit_once(',')
        .map(|(left, right)| (left.to_string(), right.to_string()))
}

#[derive(Clone, Debug, Default)]
struct LegacyAgeRatingFilter {
    ratings: Vec<u16>,
    include_null: bool,
}

fn parse_legacy_age_rating_filter(values: Option<&Vec<String>>) -> Option<LegacyAgeRatingFilter> {
    let values = values?;
    let mut filter = LegacyAgeRatingFilter::default();
    for value in values {
        match value.parse::<u16>() {
            Ok(value) => filter.ratings.push(value),
            Err(_) => filter.include_null = true,
        }
    }

    if filter.ratings.is_empty() && !filter.include_null {
        None
    } else {
        Some(filter)
    }
}

fn apply_legacy_age_rating_filter(
    page: PageEnvelope<PersistedSeriesSummary>,
    filter: &LegacyAgeRatingFilter,
    requested_page: usize,
    requested_size: usize,
    unpaged: bool,
) -> PageEnvelope<PersistedSeriesSummary> {
    let filtered = page
        .content
        .into_iter()
        .filter(|row| {
            row.age_rating
                .map(|rating| filter.ratings.contains(&rating))
                .unwrap_or(filter.include_null)
        })
        .collect::<Vec<_>>();

    let total_elements = filtered.len();
    if unpaged {
        return PageEnvelope::from_slice(filtered, 0, total_elements.max(1), total_elements);
    }

    let offset = requested_page.saturating_mul(requested_size);
    let content = if offset >= total_elements {
        vec![]
    } else {
        filtered
            .into_iter()
            .skip(offset)
            .take(requested_size)
            .collect::<Vec<_>>()
    };

    PageEnvelope::from_slice(content, requested_page, requested_size, total_elements)
}

fn series_page_response(
    page: PageEnvelope<PersistedSeriesSummary>,
    unpaged: bool,
    sorted: bool,
) -> Response {
    let mut response = Json(series_page_payload(page, !unpaged, sorted)).into_response();
    mark_runtime_owned(&mut response);
    response
}

fn normalize_kotlin_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    let normalized_size = page.total_elements.max(20);
    page.page = 0;
    page.size = normalized_size;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / normalized_size) + 1
    };
    page
}

fn author_query_to_filter_token(value: String) -> Option<String> {
    let encoded = author_query_to_author_match(value);
    let object = encoded.as_object()?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase());
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(|value| value.to_ascii_lowercase());

    match (name, role) {
        (Some(name), Some(role)) => Some(format!("{name}::{role}")),
        (Some(name), None) => Some(name),
        (None, Some(role)) => Some(format!("::{role}")),
        (None, None) => None,
    }
}

fn parse_legacy_persisted_series_sort_modes(
    sorts: &[String],
    search: Option<&str>,
    collection_ids: Option<&Vec<String>>,
) -> Vec<PersistedSeriesSortMode> {
    let mut sort_modes =
        parse_persisted_series_sort_modes(sorts, if sorts.is_empty() { search } else { None });

    if collection_ids.map(|ids| ids.is_empty()).unwrap_or(true) {
        sort_modes.retain(|mode| {
            !matches!(
                mode,
                PersistedSeriesSortMode::CollectionNumberAsc
                    | PersistedSeriesSortMode::CollectionNumberDesc
            )
        });
    }

    sort_modes
}

fn empty_series_page_response(page: usize, size: usize, unpaged: bool, sorted: bool) -> Response {
    Json(series_page_payload(
        PageEnvelope {
            content: vec![],
            page,
            size,
            total_elements: 0,
            total_pages: 0,
        },
        !unpaged,
        sorted,
    ))
    .into_response()
}

async fn series_feed(
    app: &HttpAppState,
    headers: HeaderMap,
    uri: Uri,
    sort_mode: PersistedSeriesSortMode,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let library_ids = requested_query_values(query, "library_id");
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let requested_page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let deleted = match optional_query_bool(query, "deleted") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let oneshot = match optional_query_bool(query, "oneshot") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };

    match load_persisted_series_page(
        app.services.discovery_persisted.as_ref(),
        &context,
        PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria {
                library_ids,
                deleted,
                oneshot,
                exclude_newly_added,
                ..SeriesFilterCriteria::default()
            },
            None,
            requested_page,
            size,
            unpaged,
            vec![sort_mode],
        ),
    )
    .await
    {
        Ok(page) => {
            let (page, paged) = if unpaged && kotlin_unpaged_page_shape {
                (normalize_kotlin_unpaged_page_shape(page), true)
            } else {
                (page, !unpaged)
            };
            Json(series_page_payload(page, paged, true)).into_response()
        }
        Err(error) => internal_error_response(error),
    }
}

fn should_use_strict_runtime_shape(payload: Option<&Value>) -> bool {
    payload
        .and_then(|value| value.get("condition"))
        .and_then(|condition| condition.get("type"))
        .is_some()
}

pub async fn series_latest(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    series_feed(
        app,
        headers,
        uri,
        PersistedSeriesSortMode::LastModifiedDesc,
        false,
        false,
    )
    .await
}

pub async fn series_deprecated_get(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.services.discovery_persisted.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await;
    let collection_ids = decoded_query_values(query, "collection_id");
    let metadata_status = decoded_query_values(query, "status");
    let read_status = decoded_query_values(query, "read_status");
    let publishers = decoded_query_values(query, "publisher");
    let languages = decoded_query_values(query, "language");
    let genres = decoded_query_values(query, "genre").map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>()
    });
    let tags = decoded_query_values(query, "tag").map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>()
    });
    let age_ratings = decoded_query_values(query, "age_rating");
    let legacy_age_ratings = parse_legacy_age_rating_filter(age_ratings.as_ref());
    let release_years = decoded_query_values(query, "release_year");
    let sharing_labels = decoded_query_values(query, "sharing_label").map(|values| {
        values
            .into_iter()
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>()
    });
    let authors = decoded_query_values(query, "author");
    let search_regex = decoded_delimited_pair(query, "search_regex");
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let requested_page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let deleted = match optional_query_bool(query, "deleted") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let oneshot = match optional_query_bool(query, "oneshot") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let complete = match optional_query_bool(query, "complete") {
        Ok(value) => value,
        Err(()) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sorts = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let requested_non_empty_library_ids = requested_library_ids
        .as_ref()
        .is_some_and(|values| !values.is_empty());
    if requested_non_empty_library_ids && library_ids.is_none() {
        return empty_series_page_response(requested_page, size, unpaged, false);
    }

    let (titles_regex, title_sorts_regex) = match search_regex.as_ref() {
        Some((pattern, field)) if field.eq_ignore_ascii_case("title") => {
            (Some(vec![pattern.clone()]), None)
        }
        Some((pattern, field)) if field.eq_ignore_ascii_case("title_sort") => {
            (None, Some(vec![pattern.clone()]))
        }
        _ => (None, None),
    };

    let requires_age_post_filter = legacy_age_ratings
        .as_ref()
        .is_some_and(|filter| filter.include_null && !filter.ratings.is_empty());
    let age_ratings_filter = legacy_age_ratings.as_ref().and_then(|filter| {
        if requires_age_post_filter || filter.ratings.is_empty() {
            None
        } else {
            Some(filter.ratings.clone())
        }
    });
    let age_ratings_null = legacy_age_ratings.as_ref().and_then(|filter| {
        if filter.include_null && filter.ratings.is_empty() {
            Some(true)
        } else {
            None
        }
    });

    let sort_modes = parse_legacy_persisted_series_sort_modes(
        &sorts,
        search.as_deref(),
        collection_ids.as_ref(),
    );
    let sorted = !sort_modes.is_empty();
    let requested_unpaged = unpaged || requires_age_post_filter;

    match load_persisted_series_page(
        app.services.discovery_persisted.as_ref(),
        &context,
        PersistedSeriesBrowseQuery::from_filters(
            SeriesFilterCriteria {
                library_ids,
                collection_ids,
                read_statuses: read_status,
                publishers,
                languages,
                deleted,
                oneshot,
                age_ratings: age_ratings_filter,
                age_ratings_null,
                genres,
                tags,
                release_date_begins_with: release_years
                    .as_ref()
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(|value| {
                                value.parse::<i32>().ok().map(|year| year.to_string())
                            })
                            .collect::<Vec<_>>()
                    })
                    .filter(|values| !values.is_empty()),
                sharing_labels,
                series_statuses: metadata_status,
                complete,
                authors: authors
                    .map(|values| {
                        values
                            .into_iter()
                            .filter_map(author_query_to_filter_token)
                            .collect::<Vec<_>>()
                    })
                    .filter(|values| !values.is_empty()),
                titles_regex,
                title_sorts_regex,
                ..SeriesFilterCriteria::default()
            },
            search.clone(),
            requested_page,
            size,
            requested_unpaged,
            sort_modes,
        ),
    )
    .await
    {
        Ok(page) => {
            let page = if let Some(filter) = legacy_age_ratings
                .as_ref()
                .filter(|_| requires_age_post_filter)
            {
                apply_legacy_age_rating_filter(page, filter, requested_page, size, unpaged)
            } else {
                page
            };
            series_page_response(page, unpaged, sorted)
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_alphabetical_groups(
    headers: HeaderMap,
    body: Value,
    app: &HttpAppState,
) -> Response {
    if !body.is_object() {
        return StatusCode::BAD_REQUEST.into_response();
    }

    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let filters = match parse_runtime_series_filters_with_mode(
        body.get("condition"),
        OperatorValidationMode::Strict,
    ) {
        Ok(filters) => filters,
        Err(error) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid series alphabetical-groups request: {error:?}") })),
            )
                .into_response();
        }
    };

    let mut filters = filters;
    filters.criteria.library_ids = remap_requested_library_ids_for_persisted(
        app.services.discovery_persisted.as_ref(),
        filters.criteria.library_ids.as_ref(),
    )
    .await;

    let full_text_search = extract_full_text_search(&body);

    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    match load_persisted_alphabetical_groups(
        app.services.discovery_persisted.as_ref(),
        &context,
        filters,
        full_text_search,
    )
    .await
    {
        Ok(groups) => Json(Value::Array(groups)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_list(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let payload = if body.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(object)) => Value::Object(object),
            Ok(_) | Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };
    let full_text_search = extract_full_text_search(&payload);
    let strict_runtime_shape = should_use_strict_runtime_shape(Some(&payload));

    if let Some(runtime_response) = runtime_owned_series_list_response(
        app.services.discovery_persisted.as_ref(),
        &headers,
        &uri,
        Some(&payload),
        full_text_search.clone(),
        &app.discovery_auth,
        &*app.services.runtime_identity,
        strict_runtime_shape,
    )
    .await
    {
        return runtime_response;
    }

    invalid_runtime_series_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime series filter combination".to_string(),
    ))
}

pub async fn series_new(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    series_feed(
        app,
        headers,
        uri,
        PersistedSeriesSortMode::CreatedDesc,
        false,
        false,
    )
    .await
}

pub async fn series_updated(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    series_feed(
        app,
        headers,
        uri,
        PersistedSeriesSortMode::LastModifiedDesc,
        true,
        true,
    )
    .await
}
