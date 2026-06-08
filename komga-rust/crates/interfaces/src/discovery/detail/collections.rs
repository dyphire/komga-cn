use super::collections_support::PersistedCollectionWriteInput;
use super::*;
use crate::discovery::persisted::common_helpers::decode_query_component;
use crate::discovery::series::series_read_model_page_payload;
use crate::discovery::series_routes::author_query_to_author_match;
use crate::helpers::{to_domain_query_context, validation_error_response};
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::DiscoveryState;
use axum::body::{Body, to_bytes};
use axum::extract::State;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use komga_application::discovery::{
    PageRequest, SeriesBrowseRequest, parse_series_filter_from_json,
};
use std::collections::{BTreeSet, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

struct CollectionPatchInput {
    name: Option<String>,
    ordered: Option<bool>,
    series_ids: Option<Vec<String>>,
}

pub async fn collection_series(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
    uri: Uri,
) -> Response {
    let visible_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let query_string = uri.query().unwrap_or_default();
    let collection = match load_persisted_collection_detail(&app, &collection_id).await {
        Ok(Some(collection)) => collection,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };
    let visible_series_ids =
        match visible_collection_series_ids(&app, &visible_context, &collection).await {
            Ok(ids) => ids,
            Err(error) => return internal_error_response(error),
        };
    if visible_series_ids.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let mut conditions = vec![json!({
        "type": "CollectionId",
        "operator": "is",
        "value": collection_id,
    })];

    push_any_of_series_string_conditions(&mut conditions, query_string, "library_id", "LibraryId");
    push_any_of_series_string_conditions(&mut conditions, query_string, "status", "SeriesStatus");
    push_any_of_series_string_conditions(
        &mut conditions,
        query_string,
        "read_status",
        "ReadStatus",
    );
    push_any_of_series_string_conditions(&mut conditions, query_string, "publisher", "Publisher");
    push_any_of_series_string_conditions(&mut conditions, query_string, "language", "Language");
    push_any_of_series_string_conditions(&mut conditions, query_string, "genre", "Genre");
    push_any_of_series_string_conditions(&mut conditions, query_string, "tag", "Tag");

    let age_ratings = decoded_query_values(query_string, "age_rating");
    if !age_ratings.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": age_ratings.into_iter().map(|value| {
                match value.parse::<u16>() {
                    Ok(value) => json!({"type": "AgeRating", "operator": "is", "value": value}),
                    Err(_) => json!({"type": "AgeRating", "operator": "isNull"}),
                }
            }).collect::<Vec<_>>()
        }));
    }

    let release_years = decoded_query_values(query_string, "release_year");
    if !release_years.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": release_years.into_iter().filter_map(|value| value.parse::<i32>().ok()).map(|year| {
                let after = format!("{}-12-31T12:00:00Z", year - 1);
                let before = format!("{}-01-01T12:00:00Z", year + 1);
                json!({
                    "type": "AllOfSeries",
                    "conditions": [
                        {"type": "ReleaseDate", "operator": "after", "value": after},
                        {"type": "ReleaseDate", "operator": "before", "value": before}
                    ]
                })
            }).collect::<Vec<_>>()
        }));
    }

    let authors = decoded_query_values(query_string, "author");
    if !authors.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": authors.into_iter().map(|value| {
                json!({"type": "Author", "operator": "is", "value": author_query_to_author_match(value)})
            }).collect::<Vec<_>>()
        }));
    }

    if let Some(deleted) = query_bool_option(query_string, "deleted") {
        conditions.push(json!({
            "type": "Deleted",
            "operator": if deleted { "isTrue" } else { "isFalse" },
        }));
    }
    if let Some(complete) = query_bool_option(query_string, "complete") {
        conditions.push(json!({
            "type": "Complete",
            "operator": if complete { "isTrue" } else { "isFalse" },
        }));
    }

    let body = json!({
        "condition": {
            "type": "AllOfSeries",
            "conditions": conditions,
        }
    });

    let filter = match parse_series_filter_from_json(body.get("condition")) {
        Ok(f) => f,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid series filter: {e:?}") })),
            )
                .into_response();
        }
    };

    let unpaged = collection.ordered || query_bool(query_string, "unpaged");
    let page = query_value(query_string, "page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);

    let domain_context = to_domain_query_context(visible_context);

    let result = match app
        .discovery_browse
        .list_series(
            &domain_context,
            SeriesBrowseRequest {
                filter,
                sort: vec![],
                search: None,
                page: PageRequest {
                    page,
                    size,
                    unpaged,
                },
            },
        )
        .await
    {
        Ok(page) => page,
        Err(e) => return internal_error_response(format!("{e:?}")),
    };

    let response = Json(series_read_model_page_payload(result, !unpaged, false)).into_response();

    if !collection.ordered {
        return response;
    }

    ordered_collection_series_response(response, &visible_series_ids, &uri).await
}

async fn visible_collection_series_ids(
    app: &DiscoveryState,
    context: &DiscoveryQueryContext,
    collection: &CollectionReadModel,
) -> Result<Vec<String>, String> {
    let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
    for series_id in &collection.series_ids {
        if series_visible_to_context(app, context, series_id, None).await? {
            visible_series_ids.push(series_id.clone());
        }
    }
    Ok(visible_series_ids)
}

async fn ordered_collection_series_response(
    response: Response,
    visible_series_ids: &[String],
    original_uri: &Uri,
) -> Response {
    if response.status() != StatusCode::OK {
        return response;
    }

    let requested_unpaged = query_bool(original_uri.query().unwrap_or_default(), "unpaged");
    let requested_page = query_value(original_uri.query().unwrap_or_default(), "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let requested_size = query_value(original_uri.query().unwrap_or_default(), "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);

    let (parts, body) = response.into_parts();
    let bytes = match to_bytes(body, usize::MAX).await {
        Ok(bytes) => bytes,
        Err(_) => return Response::from_parts(parts, Body::empty()),
    };
    let mut payload = match serde_json::from_slice::<Value>(&bytes) {
        Ok(payload) => payload,
        Err(_) => return Response::from_parts(parts, Body::from(bytes)),
    };

    let Some(content) = payload.get_mut("content").and_then(Value::as_array_mut) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    let order = visible_series_ids
        .iter()
        .enumerate()
        .map(|(index, series_id)| (series_id.as_str(), index))
        .collect::<HashMap<_, _>>();
    content.sort_by_key(|entry| {
        entry
            .get("id")
            .and_then(Value::as_str)
            .and_then(|id| order.get(id).copied())
            .unwrap_or(usize::MAX)
    });

    if !requested_unpaged {
        apply_ordered_collection_series_pagination(&mut payload, requested_page, requested_size);
    }

    Response::from_parts(parts, Body::from(payload.to_string()))
}

fn apply_ordered_collection_series_pagination(payload: &mut Value, page: usize, size: usize) {
    let Some(content) = payload.get_mut("content").and_then(Value::as_array_mut) else {
        return;
    };

    let total_elements = content.len();
    let offset = page.saturating_mul(size);
    let paged_content = if offset >= total_elements {
        vec![]
    } else {
        content
            .iter()
            .skip(offset)
            .take(size)
            .cloned()
            .collect::<Vec<_>>()
    };
    *content = paged_content;

    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(size)
    };
    let number_of_elements = content.len();
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    payload["pageable"]["pageNumber"] = json!(page);
    payload["pageable"]["pageSize"] = json!(size);
    payload["pageable"]["offset"] = json!(offset);
    payload["pageable"]["paged"] = Value::Bool(true);
    payload["pageable"]["unpaged"] = Value::Bool(false);
    payload["last"] = Value::Bool(last);
    payload["totalElements"] = json!(total_elements);
    payload["totalPages"] = json!(total_pages);
    payload["first"] = Value::Bool(first);
    payload["size"] = json!(size);
    payload["number"] = json!(page);
    payload["numberOfElements"] = json!(number_of_elements);
    payload["empty"] = Value::Bool(number_of_elements == 0);
}

fn decoded_query_values(query: &str, key: &str) -> Vec<String> {
    query_values(query, key)
        .into_iter()
        .map(decode_query_component)
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn push_any_of_series_string_conditions(
    conditions: &mut Vec<Value>,
    query: &str,
    key: &str,
    condition_type: &str,
) {
    let values = decoded_query_values(query, key);
    if values.is_empty() {
        return;
    }

    conditions.push(json!({
        "type": "AnyOfSeries",
        "conditions": values.into_iter().map(|value| {
            json!({
                "type": condition_type,
                "operator": "is",
                "value": value,
            })
        }).collect::<Vec<_>>()
    }));
}

fn query_bool_option(query: &str, key: &str) -> Option<bool> {
    query_value(query, key).and_then(|value| {
        if value.eq_ignore_ascii_case("true") {
            Some(true)
        } else if value.eq_ignore_ascii_case("false") {
            Some(false)
        } else {
            None
        }
    })
}

pub async fn collections(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query_string = uri.query().unwrap_or_default();
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let search = query_value(query_string, "search")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let requested_library_ids = query_values(query_string, "library_id")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let unpaged = query_bool(query_string, "unpaged");

    let visible_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let persisted_rows_exist = match persisted_collections_exist(&app).await {
        Ok(exists) => exists,
        Err(error) => return internal_error_response(error),
    };

    let mut content = if persisted_rows_exist {
        match load_persisted_collections(&app).await {
            Ok(collections) => collections,
            Err(error) => return internal_error_response(error),
        }
    } else {
        vec![]
    };
    let search_limit = content.len().max(1);
    let request_scope_context = if requested_library_ids.is_empty() {
        None
    } else {
        match app
            .discovery_auth
            .resolve_query_context_with_persistence(
                &app.identity,
                &headers,
                Some(&requested_library_ids),
            )
            .await
        {
            Some(context) => Some(context),
            None => return StatusCode::UNAUTHORIZED.into_response(),
        }
    };

    for collection in &mut content {
        let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
        let mut matches_requested_scope = request_scope_context.is_none();
        for series_id in &collection.series_ids {
            let series_library_id = match load_series_library_id(&app, series_id).await {
                Ok(Some(value)) => value,
                Ok(None) => continue,
                Err(error) => return internal_error_response(error),
            };

            if let Some(request_context) = request_scope_context.as_ref()
                && !matches_requested_scope
            {
                match series_visible_to_context(
                    &app,
                    request_context,
                    series_id,
                    Some(series_library_id.as_str()),
                )
                .await
                {
                    Ok(true) => matches_requested_scope = true,
                    Ok(false) => {}
                    Err(error) => return internal_error_response(error),
                }
            }

            match series_visible_to_context(
                &app,
                &visible_context,
                series_id,
                Some(series_library_id.as_str()),
            )
            .await
            {
                Ok(true) => visible_series_ids.push(series_id.clone()),
                Ok(false) => {}
                Err(error) => return internal_error_response(error),
            }
        }

        if visible_series_ids.len() != collection.series_ids.len() {
            collection.filtered = true;
        }
        collection.series_ids = if matches_requested_scope {
            visible_series_ids
        } else {
            vec![]
        };
    }

    content.retain(|collection| !collection.series_ids.is_empty());

    if let Some(search) = search.as_ref() {
        let ranked_ids: Vec<String> = match app
            .collection_search
            .search_collection_ids(search, search_limit)
            .await
        {
            Ok(ids) => ids,
            Err(error) => return internal_error_response(error),
        };
        let ranks = ranked_ids
            .iter()
            .enumerate()
            .map(|(index, id)| (id.as_str(), index))
            .collect::<HashMap<&str, usize>>();
        content.retain(|collection| ranks.contains_key(collection.id.as_str()));
        content.sort_by_key(|collection| {
            ranks
                .get(collection.id.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
    } else {
        let collator = collections_unicode_collator();
        content.sort_by(|left, right| collator.compare(left.name.as_str(), right.name.as_str()));
    }

    if unpaged {
        let payload = collections_unpaged_payload(content);
        return Json(payload).into_response();
    }

    let page_size = if size == 0 { 20 } else { size };
    let total_elements = content.len();
    let offset = page.saturating_mul(page_size);
    let page_content = if offset >= total_elements {
        vec![]
    } else {
        content
            .into_iter()
            .skip(offset)
            .take(page_size)
            .collect::<Vec<_>>()
    };
    let page = PageEnvelope::from_slice(page_content, page, page_size, total_elements);

    Json(collections_page_payload(page)).into_response()
}

pub async fn collection_create(
    State(app): State<DiscoveryState>,
    _: Admin,
    body: Bytes,
) -> Response {
    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = match parse_collection_create_input(&payload) {
        Ok(input) => input,
        Err(response) => return response,
    };

    match load_persisted_collections(&app).await {
        Ok(collections)
            if collections
                .iter()
                .any(|collection| collection.name.eq_ignore_ascii_case(&input.name)) =>
        {
            return collection_create_bad_request("Collection name already exists");
        }
        Ok(_) => {}
        Err(error) => return internal_error_response(error),
    }

    let created_id = match persist_collection_create(&app, &input).await {
        Ok(id) => id,
        Err(error) => return internal_error_response(error),
    };

    if let Err(error) = upsert_collection_search_document(&app, &created_id).await {
        return internal_error_response(error);
    }

    match load_persisted_collection_detail(&app, &created_id).await {
        Ok(Some(collection)) => Json(collection_payload(&collection)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

#[allow(clippy::result_large_err)]
fn parse_collection_create_input(
    payload: &Value,
) -> Result<PersistedCollectionWriteInput, Response> {
    let Some(payload) = payload.as_object() else {
        return Err(collection_create_bad_request(
            "Request body must be a JSON object",
        ));
    };

    let name = match payload.get("name") {
        Some(value) => match value.as_str() {
            Some(value) => value,
            None => return Err(collection_create_bad_request("name must be a string")),
        },
        None => {
            return Err(collection_create_bad_request(
                "Required field 'name' is not present",
            ));
        }
    };
    let ordered = match payload.get("ordered") {
        Some(value) => match value.as_bool() {
            Some(value) => value,
            None => return Err(collection_create_bad_request("ordered must be a boolean")),
        },
        None => {
            return Err(collection_create_bad_request(
                "Required field 'ordered' is not present",
            ));
        }
    };
    let series_values = match payload.get("seriesIds") {
        Some(value) => match value.as_array() {
            Some(value) => value,
            None => return Err(collection_create_bad_request("seriesIds must be an array")),
        },
        None => {
            return Err(collection_create_bad_request(
                "Required field 'seriesIds' is not present",
            ));
        }
    };

    let mut violations = Vec::new();
    if name.trim().is_empty() {
        violations.push(json!({
            "fieldName": "name",
            "message": "must not be blank",
        }));
    }
    if series_values.is_empty() {
        violations.push(json!({
            "fieldName": "seriesIds",
            "message": "must not be empty",
        }));
    }

    let mut seen_series_ids = BTreeSet::new();
    let mut series_ids = Vec::with_capacity(series_values.len());
    let mut saw_duplicate_series_id = false;
    for value in series_values {
        let Some(series_id) = value.as_str() else {
            return Err(collection_create_bad_request(
                "seriesIds must be an array of strings",
            ));
        };
        let series_id = series_id.to_string();
        if !seen_series_ids.insert(series_id.clone()) {
            saw_duplicate_series_id = true;
            continue;
        }
        series_ids.push(series_id);
    }

    if saw_duplicate_series_id {
        violations.push(json!({
            "fieldName": "seriesIds",
            "message": "must only contain unique elements",
        }));
    }

    if !violations.is_empty() {
        return Err(validation_error_response(violations));
    }

    Ok(PersistedCollectionWriteInput {
        name: name.to_string(),
        ordered,
        series_ids,
    })
}

fn collection_create_bad_request(message: &str) -> Response {
    collection_bad_request("/api/v1/collections", message)
}

fn collection_update_bad_request(collection_id: &str, message: &str) -> Response {
    collection_bad_request(&format!("/api/v1/collections/{collection_id}"), message)
}

fn collection_bad_request(path: &str, message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({
            "error": "Bad Request",
            "message": message,
            "path": path,
            "status": 400,
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        })),
    )
        .into_response()
}

#[allow(clippy::result_large_err)]
fn parse_collection_update_input(
    payload: &Value,
    request_path: &str,
) -> Result<CollectionPatchInput, Response> {
    let Some(payload) = payload.as_object() else {
        return Err(collection_bad_request(
            request_path,
            "Request body must be a JSON object",
        ));
    };

    let name = match payload.get("name") {
        Some(Value::Null) | None => None,
        Some(value) => match value.as_str() {
            Some(value) => Some(value.to_string()),
            None => {
                return Err(collection_bad_request(
                    request_path,
                    "name must be a string",
                ));
            }
        },
    };
    let ordered = match payload.get("ordered") {
        Some(Value::Null) | None => None,
        Some(value) => match value.as_bool() {
            Some(value) => Some(value),
            None => {
                return Err(collection_bad_request(
                    request_path,
                    "ordered must be a boolean",
                ));
            }
        },
    };
    let series_values = match payload.get("seriesIds") {
        Some(Value::Null) | None => None,
        Some(value) => match value.as_array() {
            Some(value) => Some(value),
            None => {
                return Err(collection_bad_request(
                    request_path,
                    "seriesIds must be an array",
                ));
            }
        },
    };

    let mut violations = Vec::new();
    if name.as_ref().is_some_and(|value| value.trim().is_empty()) {
        violations.push(json!({
            "fieldName": "name",
            "message": "must not be blank",
        }));
    }

    let series_ids = match series_values {
        Some(series_values) => {
            if series_values.is_empty() {
                violations.push(json!({
                    "fieldName": "seriesIds",
                    "message": "must not be empty",
                }));
            }

            let mut seen_series_ids = BTreeSet::new();
            let mut parsed_series_ids = Vec::with_capacity(series_values.len());
            let mut saw_duplicate_series_id = false;
            for value in series_values {
                let Some(series_id) = value.as_str() else {
                    return Err(collection_bad_request(
                        request_path,
                        "seriesIds must be an array of strings",
                    ));
                };
                let series_id = series_id.to_string();
                if !seen_series_ids.insert(series_id.clone()) {
                    saw_duplicate_series_id = true;
                    continue;
                }
                parsed_series_ids.push(series_id);
            }

            if saw_duplicate_series_id {
                violations.push(json!({
                    "fieldName": "seriesIds",
                    "message": "must only contain unique elements",
                }));
            }

            Some(parsed_series_ids)
        }
        None => None,
    };

    if !violations.is_empty() {
        return Err(validation_error_response(violations));
    }

    Ok(CollectionPatchInput {
        name,
        ordered,
        series_ids,
    })
}

fn merge_collection_patch_input(
    existing: &CollectionReadModel,
    patch: CollectionPatchInput,
) -> PersistedCollectionWriteInput {
    PersistedCollectionWriteInput {
        name: patch.name.unwrap_or_else(|| existing.name.clone()),
        ordered: patch.ordered.unwrap_or(existing.ordered),
        series_ids: patch
            .series_ids
            .unwrap_or_else(|| existing.series_ids.clone()),
    }
}

fn collection_names_equal_ignore_case(left: &str, right: &str) -> bool {
    left.to_lowercase() == right.to_lowercase()
}

fn collections_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for collections sorting should construct")
}

pub async fn collection_detail(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    match load_persisted_collection_detail(&app, &collection_id).await {
        Ok(Some(mut collection)) => {
            let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
            for series_id in &collection.series_ids {
                match series_visible_to_context(&app, &context, series_id, None).await {
                    Ok(true) => visible_series_ids.push(series_id.clone()),
                    Ok(false) => {}
                    Err(error) => return internal_error_response(error),
                }
            }

            if collection.series_ids.len() != visible_series_ids.len() {
                collection.filtered = true;
            }
            collection.series_ids = visible_series_ids;

            if collection.series_ids.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
            }

            return Json(collection_payload(&collection)).into_response();
        }
        Ok(None) => {}
        Err(error) => return internal_error_response(error),
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn collection_update(
    State(app): State<DiscoveryState>,
    _: Admin,
    Path(collection_id): Path<String>,
    body: Bytes,
) -> Response {
    let request_path = format!("/api/v1/collections/{collection_id}");
    let payload = match serde_json::from_slice::<Value>(&body) {
        Ok(value) => value,
        Err(_) => {
            return collection_bad_request(&request_path, "Request body must be a JSON object");
        }
    };
    let patch = match parse_collection_update_input(&payload, &request_path) {
        Ok(input) => input,
        Err(response) => return response,
    };
    let existing = match load_persisted_collection_detail(&app, &collection_id).await {
        Ok(Some(collection)) => collection,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };
    let should_validate_duplicate_name = patch
        .name
        .as_ref()
        .is_some_and(|name| !collection_names_equal_ignore_case(name, &existing.name));
    let input = merge_collection_patch_input(&existing, patch);

    if should_validate_duplicate_name {
        match load_persisted_collections(&app).await {
            Ok(collections)
                if collections.iter().any(|collection| {
                    collection.id != collection_id
                        && collection_names_equal_ignore_case(&collection.name, &input.name)
                }) =>
            {
                return collection_update_bad_request(
                    &collection_id,
                    "Collection name already exists",
                );
            }
            Ok(_) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    match persist_collection_update(&app, &collection_id, &input).await {
        Ok(true) => {
            if let Err(error) = upsert_collection_search_document(&app, &collection_id).await {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_delete(
    State(app): State<DiscoveryState>,
    _: Admin,
    Path(collection_id): Path<String>,
) -> Response {
    match delete_persisted_collection(&app, &collection_id).await {
        Ok(true) => {
            if let Err(error) = delete_collection_search_document(&app, &collection_id).await {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
