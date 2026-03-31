use super::*;
use crate::http::discovery::series::series_list;
use crate::http::discovery::series_routes::{author_query_to_author_match, decode_query_component};

pub async fn collection_series(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let query_string = uri.query().unwrap_or_default();
    if auth_db.database_file.exists()
        && let Ok(None) =
            load_persisted_collection_detail(auth_db.database_file.as_path(), &collection_id).await
    {
        return StatusCode::NOT_FOUND.into_response();
    } else if let Err(error) =
        load_persisted_collection_detail(auth_db.database_file.as_path(), &collection_id).await
    {
        return internal_error_response(error);
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

    let mut body = json!({
        "condition": {
            "type": "AllOfSeries",
            "conditions": conditions,
        }
    });

    if let Some(search) = query_value(query_string, "search")
        .map(decode_query_component)
        .filter(|value| !value.trim().is_empty())
    {
        body["fullTextSearch"] = Value::String(search);
    }

    series_list(
        Extension(auth_db),
        Extension(auth_state),
        headers,
        uri,
        Bytes::from(body.to_string()),
    )
    .await
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

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
        .map(|value| value.to_ascii_lowercase());
    let requested_library_ids = query_values(query_string, "library_id");
    let unpaged = query_bool(query_string, "unpaged");

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let mut content = if auth_db.database_file.exists() {
        let persisted_rows_exist =
            match persisted_collections_exist(auth_db.database_file.as_path()).await {
                Ok(exists) => exists,
                Err(error) => return internal_error_response(error),
            };

        if persisted_rows_exist {
            match load_persisted_collections(auth_db.database_file.as_path()).await {
                Ok(collections) => collections,
                Err(error) => return internal_error_response(error),
            }
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    for collection in &mut content {
        let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
        for series_id in &collection.series_ids {
            if !requested_library_ids.is_empty() {
                let series_library_id = match load_series_library_id(
                    auth_db.database_file.as_path(),
                    series_id,
                )
                .await
                {
                    Ok(Some(value)) => value,
                    Ok(None) => continue,
                    Err(error) => return internal_error_response(error),
                };

                if !requested_library_ids
                    .iter()
                    .any(|candidate| candidate == &series_library_id)
                {
                    continue;
                }
            }

            match series_visible_to_context(
                auth_db.database_file.as_path(),
                &context,
                series_id,
                None,
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
        collection.series_ids = visible_series_ids;
    }

    content.retain(|collection| !collection.series_ids.is_empty());

    if let Some(search) = search.as_ref() {
        content.retain(|collection| collection.name.to_ascii_lowercase().contains(search));
        content.sort_by(|left, right| {
            let left_name = left.name.to_ascii_lowercase();
            let right_name = right.name.to_ascii_lowercase();
            let left_rank = left_name.find(search).unwrap_or(usize::MAX);
            let right_rank = right_name.find(search).unwrap_or(usize::MAX);
            left_rank
                .cmp(&right_rank)
                .then_with(|| left_name.cmp(&right_name))
        });
    } else {
        content.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
    }

    if unpaged {
        let payload = collections_unpaged_payload(content);
        let mut response = Json(payload).into_response();
        mark_runtime_owned(&mut response);
        return response;
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

    let mut response = Json(collections_page_payload(page)).into_response();
    mark_runtime_owned(&mut response);
    response
}

pub async fn collection_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<crate::http::state::OperationalState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = collection_write_input(&payload);

    let created_id = match persist_collection_create(auth_db.database_file.as_path(), &input).await
    {
        Ok(id) => id,
        Err(error) => return internal_error_response(error),
    };

    if let Err(error) = upsert_collection_search_document(
        auth_db.database_file.as_path(),
        operational.runtime.lucene_data_directory.as_path(),
        &created_id,
    )
    .await
    {
        return internal_error_response(error);
    }

    match load_persisted_collection_detail(auth_db.database_file.as_path(), &created_id).await {
        Ok(Some(collection)) => Json(collection_payload(&collection)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if auth_db.database_file.exists() {
        match load_persisted_collection_detail(auth_db.database_file.as_path(), &collection_id)
            .await
        {
            Ok(Some(mut collection)) => {
                let mut visible_series_ids = Vec::with_capacity(collection.series_ids.len());
                for series_id in &collection.series_ids {
                    match series_visible_to_context(
                        auth_db.database_file.as_path(),
                        &context,
                        series_id,
                        None,
                    )
                    .await
                    {
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
    }

    StatusCode::NOT_FOUND.into_response()
}

pub async fn collection_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<crate::http::state::OperationalState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = collection_write_input(&payload);

    match persist_collection_update(auth_db.database_file.as_path(), &collection_id, &input).await {
        Ok(true) => {
            if let Err(error) = upsert_collection_search_document(
                auth_db.database_file.as_path(),
                operational.runtime.lucene_data_directory.as_path(),
                &collection_id,
            )
            .await
            {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(operational): Extension<crate::http::state::OperationalState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_collection(auth_db.database_file.as_path(), &collection_id).await {
        Ok(true) => {
            if let Err(error) = delete_collection_search_document(
                operational.runtime.lucene_data_directory.as_path(),
                &collection_id,
            )
            .await
            {
                return internal_error_response(error);
            }
            StatusCode::NO_CONTENT.into_response()
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
