use super::persisted::common_helpers::{
    decode_query_component, internal_error_response, requested_query_values,
};
use super::persisted::models::PersistedSeriesSummary;
use super::persisted::series_queries::series_page_payload;
use super::series_routes::author_query_to_author_match;
use super::*;
use crate::helpers::to_domain_query_context;
use axum::extract::State;
use komga_application::discovery::{SeriesBrowseQuery, SeriesReadModel};
use komga_domain::common_ids::{CollectionId, LibraryId};
use komga_domain::discovery::PageEnvelope;
use komga_domain::discovery::{
    AgeRatingCondition, CompositeSeriesCondition, DateCondition, FilterOperator,
    InclusionCondition, ReadStatusCondition, SeriesCondition, SeriesFilter, SeriesSort,
    SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
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

fn parse_legacy_series_sorts(
    sorts: &[String],
    search: Option<&str>,
    collection_ids: Option<&Vec<String>>,
) -> Vec<SeriesSort> {
    let has_search = search.map(str::trim).filter(|v| !v.is_empty()).is_some();
    let mut result = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" | "titleSort,asc" => Some(SeriesSort::MetadataTitleSortAsc),
            "metadata.titleSort,desc" | "titleSort,desc" => Some(SeriesSort::MetadataTitleSortDesc),
            "name,asc" => Some(SeriesSort::NameAsc),
            "name,desc" => Some(SeriesSort::NameDesc),
            "readDate,asc" => Some(SeriesSort::ReadDateAsc),
            "readDate,desc" => Some(SeriesSort::ReadDateDesc),
            "collection.number,asc" => Some(SeriesSort::CollectionNumberAsc),
            "collection.number,desc" => Some(SeriesSort::CollectionNumberDesc),
            "random,asc" | "random,desc" => Some(SeriesSort::Random),
            "createdDate,asc" | "created,asc" => Some(SeriesSort::CreatedDateAsc),
            "createdDate,desc" | "created,desc" => Some(SeriesSort::CreatedDateDesc),
            "lastModifiedDate,asc" | "lastModified,asc" => Some(SeriesSort::LastModifiedDateAsc),
            "lastModifiedDate,desc" | "lastModified,desc" => Some(SeriesSort::LastModifiedDateDesc),
            "booksMetadata.releaseDate,asc" => Some(SeriesSort::ReleaseDateAsc),
            "booksMetadata.releaseDate,desc" => Some(SeriesSort::ReleaseDateDesc),
            "booksCount,asc" => Some(SeriesSort::BooksCountAsc),
            "booksCount,desc" => Some(SeriesSort::BooksCountDesc),
            "relevance,asc" if has_search => Some(SeriesSort::RelevanceAsc),
            "relevance,desc" if has_search => Some(SeriesSort::RelevanceDesc),
            _ => None,
        })
        .collect::<Vec<_>>();
    result.dedup();
    if result.is_empty() && sorts.is_empty() && has_search {
        result.push(SeriesSort::RelevanceAsc);
    }
    // Filter out CollectionNumber sorts if no collection_ids are specified
    if collection_ids.map(|ids| ids.is_empty()).unwrap_or(true) {
        result.retain(|sort| {
            !matches!(
                sort,
                SeriesSort::CollectionNumberAsc | SeriesSort::CollectionNumberDesc
            )
        });
    }
    result
}

async fn series_feed(
    app: &HttpAppState,
    headers: HeaderMap,
    uri: Uri,
    sorts: Vec<SeriesSort>,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

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

    let mut conditions = Vec::new();
    if let Some(ids) = &requested_library_ids
        && !ids.is_empty()
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::LibraryId(
            InclusionCondition::Include(ids.iter().cloned().map(LibraryId::from).collect()),
        )));
    }
    if let Some(val) = deleted {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Deleted(val)));
    }
    if let Some(val) = oneshot {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::OneShot(val)));
    }
    if exclude_newly_added {
        conditions.push(SeriesCondition::Value(
            SeriesValueCondition::ExcludeNewlyAdded(true),
        ));
    }

    let filter = SeriesFilter {
        condition: if conditions.len() == 1 {
            conditions.pop()
        } else if conditions.len() > 1 {
            Some(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::All,
                conditions,
            }))
        } else {
            None
        },
    };

    match app
        .services
        .discovery_list
        .list_series(
            &context,
            SeriesBrowseQuery {
                filter,
                sort: sorts,
                search: None,
                page: requested_page,
                size,
                unpaged,
            },
        )
        .await
    {
        Ok(page) => {
            let (page, paged) = if unpaged && kotlin_unpaged_page_shape {
                (normalize_kotlin_unpaged_page_shape(page), true)
            } else {
                (page, !unpaged)
            };
            Json(series_read_model_page_payload(page, paged, true)).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub async fn series_latest(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    series_feed(
        app,
        headers,
        uri,
        vec![SeriesSort::LastModifiedDateDesc],
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
    let collection_ids = decoded_query_values(query, "collection_id");
    let collection_ids_for_sort = collection_ids.clone();
    let metadata_status = decoded_query_values(query, "status");
    let read_status = decoded_query_values(query, "read_status");
    let publishers = decoded_query_values(query, "publisher");
    let languages = decoded_query_values(query, "language");
    let genres = decoded_query_values(query, "genre");
    let tags = decoded_query_values(query, "tag");
    let age_ratings = decoded_query_values(query, "age_rating");
    let release_years = decoded_query_values(query, "release_year");
    let sharing_labels = decoded_query_values(query, "sharing_label");
    let authors = decoded_query_values(query, "author");
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let requested_page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let deleted = optional_query_bool(query, "deleted");
    if deleted.is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let oneshot = optional_query_bool(query, "oneshot");
    if oneshot.is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let complete = optional_query_bool(query, "complete");
    if complete.is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sorts = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();

    // Build domain SeriesFilter from query params
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
    if let Some(val) = deleted.unwrap_or(None) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Deleted(val)));
    }
    if let Some(val) = oneshot.unwrap_or(None) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::OneShot(val)));
    }
    if let Some(val) = complete.unwrap_or(None) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Complete(val)));
    }
    if let Some(statuses) = metadata_status.filter(|v| !v.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::SeriesStatus(
            SeriesStatusCondition::Include(statuses),
        )));
    }
    if let Some(statuses) = read_status.filter(|v| !v.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::ReadStatus(
            ReadStatusCondition::Include(statuses),
        )));
    }
    if let Some(vals) = publishers.filter(|v| !v.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Publisher(
            InclusionCondition::Include(vals),
        )));
    }
    if let Some(vals) = languages.filter(|v| !v.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Language(
            InclusionCondition::Include(vals),
        )));
    }
    if let Some(vals) = genres
        .map(|values| {
            values
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Genre(
            StringCondition::Exact(InclusionCondition::Include(vals)),
        )));
    }
    if let Some(vals) = tags
        .map(|values| {
            values
                .into_iter()
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Tag(
            StringCondition::Exact(InclusionCondition::Include(vals)),
        )));
    }
    if let Some(values) = age_ratings.filter(|v| !v.is_empty()) {
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
    if let Some(vals) = release_years
        .map(|values| {
            values
                .into_iter()
                .filter_map(|v| v.parse::<i32>().ok().map(|year| year.to_string()))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::ReleaseDate(
            DateCondition::StartsWith(InclusionCondition::Include(vals)),
        )));
    }
    if let Some(vals) = sharing_labels.filter(|v| !v.is_empty()) {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::SharingLabel(
            StringCondition::Exact(InclusionCondition::Include(vals)),
        )));
    }
    if let Some(vals) = authors
        .map(|values| {
            values
                .into_iter()
                .filter_map(author_query_to_filter_token)
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
    {
        conditions.push(SeriesCondition::Value(SeriesValueCondition::Author(
            StringCondition::Exact(InclusionCondition::Include(vals)),
        )));
    }

    let filter = SeriesFilter {
        condition: if conditions.len() == 1 {
            conditions.pop()
        } else if conditions.len() > 1 {
            Some(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::All,
                conditions,
            }))
        } else {
            None
        },
    };

    // Convert sort strings to domain SeriesSort
    let domain_sorts =
        parse_legacy_series_sorts(&sorts, search.as_deref(), collection_ids_for_sort.as_ref());
    let sorted = !domain_sorts.is_empty();

    match app
        .services
        .discovery_list
        .list_series(
            &context,
            SeriesBrowseQuery {
                filter,
                sort: domain_sorts,
                search,
                page: requested_page,
                size,
                unpaged,
            },
        )
        .await
    {
        Ok(page) => {
            let mut response =
                Json(series_read_model_page_payload(page, !unpaged, sorted)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(e) => internal_error_response(format!("{e:?}")),
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

    let filter = match parse_series_filter_from_json(body.get("condition")) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid series alphabetical-groups request: {e:?}") })),
            )
                .into_response();
        }
    };

    let full_text_search = extract_full_text_search(&body);

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .services
        .discovery_list
        .list_series_alphabetical_groups(&context, filter, full_text_search)
        .await
    {
        Ok(groups) => Json(Value::Array(groups)).into_response(),
        Err(e) => internal_error_response(format!("{e:?}")),
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

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&*app.services.runtime_identity, &headers, None)
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let filter = match parse_series_filter_from_json(payload.get("condition")) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid series filter: {e:?}") })),
            )
                .into_response();
        }
    };

    let search = extract_full_text_search(&payload);
    let has_search = search
        .as_ref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false);
    let query = uri.query().unwrap_or_default();
    let query_sort_values = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let sort = if query_sort_values.is_empty() {
        parse_series_sorts_from_json(payload.get("sort"), has_search)
    } else {
        parse_series_sorts_from_json_values(&query_sort_values, has_search)
    };
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            payload
                .get("page")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
        })
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| {
            payload
                .get("size")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize)
        })
        .unwrap_or(20)
        .max(1);
    let unpaged = query_value(query, "unpaged")
        .map(|_| query_bool(query, "unpaged"))
        .or_else(|| payload.get("unpaged").and_then(|value| value.as_bool()))
        .unwrap_or(false);

    let sorted = !sort.is_empty();

    match app
        .services
        .discovery_list
        .list_series(
            &context,
            SeriesBrowseQuery {
                filter,
                sort,
                search,
                page,
                size,
                unpaged,
            },
        )
        .await
    {
        Ok(page) => Json(series_read_model_page_payload(page, !unpaged, sorted)).into_response(),
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

fn parse_string_value(condition: &Value, key: &str) -> Option<String> {
    condition
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parse_required_lower_string_value(
    condition: &Value,
    condition_type: &str,
) -> Result<String, DiscoveryError> {
    let value = parse_string_value(condition, "value")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if value.is_empty() {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a non-empty value",
        )));
    }
    Ok(value)
}

fn parse_series_string_condition(
    condition: &Value,
    condition_type: &str,
) -> Result<StringCondition, DiscoveryError> {
    let operator = parse_operator(condition);
    match operator.as_str() {
        "isnull" => Ok(StringCondition::IsEmpty),
        "isnotnull" => Ok(StringCondition::IsNotEmpty),
        "contains" => Ok(StringCondition::Contains(InclusionCondition::Include(
            vec![parse_required_lower_string_value(
                condition,
                condition_type,
            )?],
        ))),
        "isnot" => Ok(StringCondition::Exact(InclusionCondition::Exclude(vec![
            parse_required_lower_string_value(condition, condition_type)?,
        ]))),
        "is" => Ok(StringCondition::Exact(InclusionCondition::Include(vec![
            parse_required_lower_string_value(condition, condition_type)?,
        ]))),
        _ => Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported operator for {condition_type}: {operator}",
        ))),
    }
}

fn parse_u16_value(condition: &Value, condition_type: &str) -> Result<u16, DiscoveryError> {
    condition
        .get("value")
        .and_then(|v| {
            v.as_u64()
                .map(|n| n as u16)
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u16>().ok()))
        })
        .ok_or_else(|| {
            DiscoveryError::InvalidSemantics(format!(
                "{condition_type} filter requires a numeric value",
            ))
        })
}

fn normalize_release_date_date_time(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.len() >= 10 {
        &trimmed[..10]
    } else {
        trimmed
    };

    let bytes = candidate.as_bytes();
    if bytes.len() != 10
        || !bytes[0].is_ascii_digit()
        || !bytes[1].is_ascii_digit()
        || !bytes[2].is_ascii_digit()
        || !bytes[3].is_ascii_digit()
        || bytes[4] != b'-'
        || !bytes[5].is_ascii_digit()
        || !bytes[6].is_ascii_digit()
        || bytes[7] != b'-'
        || !bytes[8].is_ascii_digit()
        || !bytes[9].is_ascii_digit()
    {
        return None;
    }

    Some(candidate.to_string())
}

fn parse_release_date_operand(
    condition: &Value,
    condition_type: &str,
) -> Result<String, DiscoveryError> {
    if let Some(value) = parse_string_value(condition, "value")
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        return Ok(value);
    }

    if let Some(value) = condition
        .get("dateTime")
        .and_then(Value::as_str)
        .and_then(normalize_release_date_date_time)
    {
        return Ok(value);
    }

    Err(DiscoveryError::InvalidSemantics(format!(
        "{condition_type} filter requires a non-empty value",
    )))
}

fn parse_duration_days(condition: &Value, condition_type: &str) -> Result<i64, DiscoveryError> {
    let raw = condition
        .get("duration")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let Some(days) = raw
        .strip_prefix('P')
        .and_then(|value| value.strip_suffix('D'))
    else {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} duration must be an ISO-8601 day duration",
        )));
    };
    days.parse::<i64>().map_err(|_| {
        DiscoveryError::InvalidSemantics(format!(
            "{condition_type} duration must be an ISO-8601 day duration",
        ))
    })
}

fn parse_author_condition_value(condition: &Value) -> Result<String, DiscoveryError> {
    let Some(value) = condition.get("value") else {
        return Err(DiscoveryError::InvalidSemantics(
            "Author filter requires a non-empty value".to_string(),
        ));
    };

    if let Some(raw) = value.as_str() {
        let value = raw.trim().to_ascii_lowercase();
        if value.is_empty() {
            return Err(DiscoveryError::InvalidSemantics(
                "Author filter requires a non-empty value".to_string(),
            ));
        }
        return Ok(value);
    }

    let Some(object) = value.as_object() else {
        return Err(DiscoveryError::InvalidSemantics(
            "Author filter value must be a string or object".to_string(),
        ));
    };
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
        (Some(name), Some(role)) => Ok(format!("{name}::{role}")),
        (Some(name), None) => Ok(name),
        (None, Some(role)) => Ok(format!("::{role}")),
        (None, None) => Err(DiscoveryError::InvalidSemantics(
            "Author filter requires name or role".to_string(),
        )),
    }
}

fn parse_operator(condition: &Value) -> String {
    condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn parse_single_series_value_condition(
    condition: &Value,
) -> Result<SeriesValueCondition, DiscoveryError> {
    let condition_type = condition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let operator = parse_operator(condition);

    match condition_type {
        "Title" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "Title filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::Title(match operator.as_str() {
                "contains" => StringCondition::Contains(InclusionCondition::Include(vec![value])),
                "isnot" => StringCondition::Exact(InclusionCondition::Exclude(vec![value])),
                "is" => StringCondition::Exact(InclusionCondition::Include(vec![value])),
                "beginswith" => {
                    StringCondition::StartsWith(InclusionCondition::Include(vec![value]))
                }
                "endswith" => StringCondition::EndsWith(InclusionCondition::Include(vec![value])),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Title: {operator}",
                    )));
                }
            }))
        }
        "Deleted" => {
            let value = match operator.as_str() {
                "istrue" => true,
                "isfalse" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Deleted: {operator}",
                    )));
                }
            };
            Ok(SeriesValueCondition::Deleted(value))
        }
        "OneShot" => {
            let value = match operator.as_str() {
                "istrue" => true,
                "isfalse" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for OneShot: {operator}",
                    )));
                }
            };
            Ok(SeriesValueCondition::OneShot(value))
        }
        "Complete" => {
            let value = match operator.as_str() {
                "istrue" => true,
                "isfalse" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Complete: {operator}",
                    )));
                }
            };
            Ok(SeriesValueCondition::Complete(value))
        }
        "LibraryId" => {
            if operator != "is" {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for LibraryId: {operator}",
                )));
            }
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "LibraryId filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::LibraryId(
                InclusionCondition::Include(vec![LibraryId::from(value)]),
            ))
        }
        "CollectionId" => {
            if operator != "is" {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for CollectionId: {operator}",
                )));
            }
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "CollectionId filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::CollectionId(
                InclusionCondition::Include(vec![CollectionId::from(value)]),
            ))
        }
        "Genre" => Ok(SeriesValueCondition::Genre(parse_series_string_condition(
            condition, "Genre",
        )?)),
        "Tag" => Ok(SeriesValueCondition::Tag(parse_series_string_condition(
            condition, "Tag",
        )?)),
        "Language" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "Language filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::Language(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Language: {operator}",
                    )));
                }
            }))
        }
        "Publisher" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "Publisher filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::Publisher(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Publisher: {operator}",
                    )));
                }
            }))
        }
        "AgeRating" => Ok(SeriesValueCondition::AgeRating(match operator.as_str() {
            "isnull" => AgeRatingCondition::IsEmpty,
            "isnotnull" => AgeRatingCondition::IsNotEmpty,
            "isnot" => {
                AgeRatingCondition::Exact(InclusionCondition::Exclude(vec![parse_u16_value(
                    condition,
                    "AgeRating",
                )?]))
            }
            "is" => AgeRatingCondition::Exact(InclusionCondition::Include(vec![parse_u16_value(
                condition,
                "AgeRating",
            )?])),
            "greaterthan" => {
                AgeRatingCondition::GreaterThan(parse_u16_value(condition, "AgeRating")?)
            }
            "lessthan" => AgeRatingCondition::LessThan(parse_u16_value(condition, "AgeRating")?),
            _ => {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for AgeRating: {operator}",
                )));
            }
        })),
        "ReadStatus" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "ReadStatus filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::ReadStatus(match operator.as_str() {
                "isnot" => ReadStatusCondition::Exclude(vec![value]),
                "is" => ReadStatusCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for ReadStatus: {operator}",
                    )));
                }
            }))
        }
        "SharingLabel" => Ok(SeriesValueCondition::SharingLabel(
            parse_series_string_condition(condition, "SharingLabel")?,
        )),
        "SeriesStatus" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "SeriesStatus filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::SeriesStatus(
                match operator.as_str() {
                    "isnot" => SeriesStatusCondition::Exclude(vec![value]),
                    "is" => SeriesStatusCondition::Include(vec![value]),
                    _ => {
                        return Err(DiscoveryError::InvalidSemantics(format!(
                            "unsupported operator for SeriesStatus: {operator}",
                        )));
                    }
                },
            ))
        }
        "Author" => {
            let value = parse_author_condition_value(condition)?;
            Ok(SeriesValueCondition::Author(match operator.as_str() {
                "contains" => StringCondition::Contains(InclusionCondition::Include(vec![value])),
                "isnot" => StringCondition::Exact(InclusionCondition::Exclude(vec![value])),
                "is" => StringCondition::Exact(InclusionCondition::Include(vec![value])),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Author: {operator}",
                    )));
                }
            }))
        }
        "ReleaseDate" => Ok(SeriesValueCondition::ReleaseDate(match operator.as_str() {
            "isnull" => DateCondition::IsEmpty,
            "isnotnull" => DateCondition::IsNotEmpty,
            "is" => DateCondition::Exact(InclusionCondition::Include(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "isnot" => DateCondition::Exact(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "after" | "greaterthan" => {
                DateCondition::After(parse_release_date_operand(condition, "ReleaseDate")?)
            }
            "before" | "lessthan" => {
                DateCondition::Before(parse_release_date_operand(condition, "ReleaseDate")?)
            }
            "beginswith" => DateCondition::StartsWith(InclusionCondition::Include(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "endswith" => DateCondition::EndsWith(InclusionCondition::Include(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "doesnotcontain" => DateCondition::Contains(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "doesnotbeginwith" => DateCondition::StartsWith(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "doesnotendwith" => DateCondition::EndsWith(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "isinthelast" => {
                DateCondition::WithinLastDays(parse_duration_days(condition, "ReleaseDate")?)
            }
            "isnotinthelast" => {
                DateCondition::OutsideLastDays(parse_duration_days(condition, "ReleaseDate")?)
            }
            _ => {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for ReleaseDate: {operator}",
                )));
            }
        })),
        "AllOfSeries" | "AnyOfSeries" => Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} is a composite condition and must not appear in parse_single_series_value_condition",
        ))),
        other => Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported series condition type: {other}",
        ))),
    }
}

fn parse_series_condition_from_json(condition: &Value) -> Result<SeriesCondition, DiscoveryError> {
    let condition_type = condition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match condition_type {
        "AllOfSeries" => {
            let children = condition
                .get("conditions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiscoveryError::InvalidSemantics(
                        "AllOfSeries composite filter missing conditions".to_string(),
                    )
                })?;
            let conditions = children
                .iter()
                .map(parse_series_condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::All,
                conditions,
            }))
        }
        "AnyOfSeries" => {
            let children = condition
                .get("conditions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiscoveryError::InvalidSemantics(
                        "AnyOfSeries composite filter missing conditions".to_string(),
                    )
                })?;
            let conditions = children
                .iter()
                .map(parse_series_condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::Any,
                conditions,
            }))
        }
        _ => {
            let value = parse_single_series_value_condition(condition)?;
            Ok(SeriesCondition::Value(value))
        }
    }
}

pub(crate) fn parse_series_filter_from_json(
    condition: Option<&Value>,
) -> Result<SeriesFilter, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(SeriesFilter { condition: None });
    };

    let parsed = parse_series_condition_from_json(condition)?;
    Ok(SeriesFilter {
        condition: Some(parsed),
    })
}

pub(crate) fn parse_series_sorts_from_json(
    sorts: Option<&Value>,
    has_search: bool,
) -> Vec<SeriesSort> {
    let Some(sort_values) = sorts.and_then(Value::as_array) else {
        return parse_series_sorts_from_json_values(&[], has_search);
    };

    let values = sort_values
        .iter()
        .filter_map(|v| v.as_str())
        .map(str::to_string)
        .collect::<Vec<_>>();
    parse_series_sorts_from_json_values(&values, has_search)
}

fn parse_series_sorts_from_json_values(sorts: &[String], has_search: bool) -> Vec<SeriesSort> {
    let mut result = sorts
        .iter()
        .filter_map(|s| {
            let trimmed = s.trim();
            match trimmed {
                "metadata.titleSort,asc" | "titleSort,asc" => {
                    Some(SeriesSort::MetadataTitleSortAsc)
                }
                "metadata.titleSort,desc" | "titleSort,desc" => {
                    Some(SeriesSort::MetadataTitleSortDesc)
                }
                "name,asc" => Some(SeriesSort::NameAsc),
                "name,desc" => Some(SeriesSort::NameDesc),
                "createdDate,asc" | "created,asc" => Some(SeriesSort::CreatedDateAsc),
                "createdDate,desc" | "created,desc" => Some(SeriesSort::CreatedDateDesc),
                "lastModifiedDate,asc" | "lastModified,asc" => {
                    Some(SeriesSort::LastModifiedDateAsc)
                }
                "lastModifiedDate,desc" | "lastModified,desc" => {
                    Some(SeriesSort::LastModifiedDateDesc)
                }
                "releaseDate,asc" | "booksMetadata.releaseDate,asc" => {
                    Some(SeriesSort::ReleaseDateAsc)
                }
                "releaseDate,desc" | "booksMetadata.releaseDate,desc" => {
                    Some(SeriesSort::ReleaseDateDesc)
                }
                "booksCount,asc" => Some(SeriesSort::BooksCountAsc),
                "booksCount,desc" => Some(SeriesSort::BooksCountDesc),
                "collectionNumber,asc" => Some(SeriesSort::CollectionNumberAsc),
                "collectionNumber,desc" => Some(SeriesSort::CollectionNumberDesc),
                "readDate,asc" => Some(SeriesSort::ReadDateAsc),
                "readDate,desc" => Some(SeriesSort::ReadDateDesc),
                "random" => Some(SeriesSort::Random),
                "relevance,asc" if has_search => Some(SeriesSort::RelevanceAsc),
                "relevance,desc" if has_search => Some(SeriesSort::RelevanceDesc),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    result.dedup();
    if result.is_empty() && sorts.is_empty() && has_search {
        result.push(SeriesSort::RelevanceAsc);
    }
    result
}

fn series_read_model_to_persisted(model: &SeriesReadModel) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: model.id.clone(),
        library_id: String::new(),
        name: model.name.clone(),
        title: model.title.clone(),
        title_sort: model.title.clone(),
        labels: vec![],
        created: String::new(),
        last_modified: String::new(),
        file_last_modified: String::new(),
        books_count: 0,
        books_read_count: 0,
        books_unread_count: 0,
        books_in_progress_count: 0,
        status: String::new(),
        summary: String::new(),
        reading_direction: String::new(),
        publisher: String::new(),
        age_rating: None,
        language: String::new(),
        genres: vec![],
        tags: vec![],
        alternate_titles: vec![],
        metadata_created: String::new(),
        metadata_last_modified: String::new(),
        books_metadata_authors: vec![],
        books_metadata_tags: vec![],
        books_metadata_release_date: None,
        books_metadata_summary: String::new(),
        books_metadata_summary_number: String::new(),
        books_metadata_created: String::new(),
        books_metadata_last_modified: String::new(),
        deleted: false,
        oneshot: false,
    }
}

pub(super) fn series_read_model_page_payload(
    page: PageEnvelope<SeriesReadModel>,
    paged: bool,
    sorted: bool,
) -> Value {
    let converted = PageEnvelope {
        content: page
            .content
            .iter()
            .map(series_read_model_to_persisted)
            .collect(),
        page: page.page,
        size: page.size,
        total_elements: page.total_elements,
        total_pages: page.total_pages,
    };
    series_page_payload(converted, paged, sorted)
}

pub async fn series_new(headers: HeaderMap, uri: Uri, app: &HttpAppState) -> Response {
    series_feed(
        app,
        headers,
        uri,
        vec![SeriesSort::CreatedDateDesc],
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
        vec![SeriesSort::LastModifiedDateDesc],
        true,
        true,
    )
    .await
}
