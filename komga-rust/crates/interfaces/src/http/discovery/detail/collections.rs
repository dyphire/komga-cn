use super::*;

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
    let page = query_value(query_string, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query_string, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let unpaged = query_bool(query_string, "unpaged");

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if auth_db.database_file.exists() {
        match load_persisted_collection_series(auth_db.database_file.as_path(), &collection_id)
            .await
        {
            Ok(Some(series)) => {
                let mut visible_series = Vec::with_capacity(series.len());
                for entry in series {
                    match series_visible_to_context(
                        auth_db.database_file.as_path(),
                        &context,
                        &entry.id,
                        Some(&entry.library_id),
                    )
                    .await
                    {
                        Ok(true) => visible_series.push(entry),
                        Ok(false) => {}
                        Err(error) => return internal_error_response(error),
                    }
                }

                let page_payload =
                    collection_series_page_payload(visible_series, page, size, unpaged);
                return Json(page_payload).into_response();
            }
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    StatusCode::NOT_FOUND.into_response()
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

    content.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });

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
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn collection_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_collection(auth_db.database_file.as_path(), &collection_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}
