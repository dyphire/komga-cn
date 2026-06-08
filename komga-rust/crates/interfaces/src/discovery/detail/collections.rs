use super::*;
use crate::discovery::persisted::common_helpers::decode_query_component;
use crate::discovery::series::series_read_model_page_payload;
use crate::discovery::series_routes::author_query_to_author_match;
use crate::helpers::{to_domain_query_context, validation_error_response};
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::DiscoveryState;
use axum::extract::State;
use komga_application::discovery::{
    CollectionListService, CollectionMutationError, CollectionMutationInput,
    CollectionMutationService, CollectionVisibilityService, PageRequest, SeriesBrowseRequest,
    parse_series_filter_from_json, resolve_collection_list_request,
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
    let domain_context = to_domain_query_context(visible_context);
    let service =
        CollectionVisibilityService::new(app.collection.as_ref(), app.series_detail.as_ref());
    let visible_series_ids = match service
        .visible_collection_series_ids(&domain_context, &collection)
        .await
    {
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

    let requested_unpaged = query_bool(query_string, "unpaged");
    let unpaged = collection.ordered || requested_unpaged;
    let page = query_value(query_string, "page")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);

    let mut result = match app
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

    if collection.ordered {
        result = ordered_collection_series_page(
            result,
            &visible_series_ids,
            requested_unpaged,
            page,
            size,
        );
    }

    Json(series_read_model_page_payload(
        result,
        if collection.ordered {
            !requested_unpaged
        } else {
            !unpaged
        },
        false,
    ))
    .into_response()
}

fn ordered_collection_series_page(
    mut page: PageEnvelope<SeriesReadModel>,
    visible_series_ids: &[String],
    requested_unpaged: bool,
    requested_page: usize,
    requested_size: usize,
) -> PageEnvelope<SeriesReadModel> {
    let order = visible_series_ids
        .iter()
        .enumerate()
        .map(|(index, series_id)| (series_id.as_str(), index))
        .collect::<HashMap<_, _>>();

    page.content
        .sort_by_key(|series| order.get(series.id.as_str()).copied().unwrap_or(usize::MAX));

    let total_elements = page.content.len();
    if requested_unpaged {
        return PageEnvelope::from_slice(page.content, 0, total_elements.max(1), total_elements);
    }

    let requested_size = requested_size.max(1);
    let offset = requested_page.saturating_mul(requested_size);
    let paged_content = if offset >= total_elements {
        vec![]
    } else {
        page.content
            .into_iter()
            .skip(offset)
            .take(requested_size)
            .collect::<Vec<_>>()
    };
    PageEnvelope::from_slice(
        paged_content,
        requested_page,
        requested_size,
        total_elements,
    )
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
    let resolved = resolve_collection_list_request(query_string);
    let unpaged = resolved.query.unpaged;

    let visible_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let request_scope_context = if resolved.requested_library_ids.is_empty() {
        None
    } else {
        match app
            .discovery_auth
            .resolve_query_context_with_persistence(
                &app.identity,
                &headers,
                Some(&resolved.requested_library_ids),
            )
            .await
        {
            Some(context) => Some(context),
            None => return StatusCode::UNAUTHORIZED.into_response(),
        }
    };

    let service = CollectionListService::new(
        app.collection.as_ref(),
        app.series_detail.as_ref(),
        app.collection_search.as_ref(),
    );
    let domain_visible_context = to_domain_query_context(visible_context);
    let domain_request_scope_context = request_scope_context.clone().map(to_domain_query_context);
    let page = match service
        .list_collections(
            &domain_visible_context,
            domain_request_scope_context.as_ref(),
            resolved.query,
        )
        .await
    {
        Ok(page) => page,
        Err(error) => return internal_error_response(error),
    };

    if unpaged {
        let payload = collections_unpaged_payload(page.content);
        return Json(payload).into_response();
    }

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

    let service = CollectionMutationService::new(app.collection.as_ref());
    let created = match service.create_collection(input).await {
        Ok(created) => created,
        Err(error) => return collection_mutation_error_response(error, "/api/v1/collections"),
    };

    match load_persisted_collection_detail(&app, &created.collection_id).await {
        Ok(Some(collection)) => Json(collection_payload(&collection)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

#[allow(clippy::result_large_err)]
fn parse_collection_create_input(payload: &Value) -> Result<CollectionMutationInput, Response> {
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

    Ok(CollectionMutationInput {
        name: name.to_string(),
        ordered,
        series_ids,
    })
}

fn collection_create_bad_request(message: &str) -> Response {
    collection_bad_request("/api/v1/collections", message)
}

fn collection_mutation_error_response(error: CollectionMutationError, path: &str) -> Response {
    match error {
        CollectionMutationError::DuplicateName => {
            collection_bad_request(path, "Collection name already exists")
        }
        CollectionMutationError::Persistence(error) => internal_error_response(error),
    }
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
) -> CollectionMutationInput {
    CollectionMutationInput {
        name: patch.name.unwrap_or_else(|| existing.name.clone()),
        ordered: patch.ordered.unwrap_or(existing.ordered),
        series_ids: patch
            .series_ids
            .unwrap_or_else(|| existing.series_ids.clone()),
    }
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

    let service =
        CollectionVisibilityService::new(app.collection.as_ref(), app.series_detail.as_ref());
    match service
        .collection_detail(&to_domain_query_context(context), &collection_id)
        .await
    {
        Ok(Some(collection)) => Json(collection_payload(&collection)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
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
    let input = merge_collection_patch_input(&existing, patch);

    let service = CollectionMutationService::new(app.collection.as_ref());
    let path = format!("/api/v1/collections/{collection_id}");
    match service.update_collection(&collection_id, input).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => collection_mutation_error_response(error, path.as_str()),
    }
}

pub async fn collection_delete(
    State(app): State<DiscoveryState>,
    _: Admin,
    Path(collection_id): Path<String>,
) -> Response {
    let service = CollectionMutationService::new(app.collection.as_ref());
    match service.delete_collection(&collection_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error.to_string()),
    }
}
