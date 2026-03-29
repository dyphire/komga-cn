use super::*;

pub async fn authors(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_authors(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn authors_names(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let search = query_value(uri.query().unwrap_or_default(), "search")
        .map(decode_query_component)
        .unwrap_or_default();

    match load_persisted_author_names(database_file, &search).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn authors_roles(headers: HeaderMap, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    match load_persisted_author_roles(database_file).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn authors_v2(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let search = query_value(query, "search")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let role = query_value(query, "role")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let library_ids = query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let collection_id = query_value(query, "collection_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let series_id = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let readlist_id = query_value(query, "readlist_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let scope = if !library_ids.is_empty() {
        PersistedAuthorsScope::Libraries(library_ids)
    } else if let Some(collection_id) = collection_id {
        PersistedAuthorsScope::Collection(collection_id)
    } else if let Some(series_id) = series_id {
        PersistedAuthorsScope::Series(series_id)
    } else if let Some(readlist_id) = readlist_id {
        PersistedAuthorsScope::ReadList(readlist_id)
    } else {
        PersistedAuthorsScope::All
    };

    let mut authors = match load_persisted_authors_by_scope(database_file, &scope).await {
        Ok(values) => values,
        Err(error) => return internal_error_response(error),
    };

    if let Some(role) = role {
        let role = role.to_ascii_lowercase();
        authors.retain(|author| author.role.to_ascii_lowercase() == role);
    }

    if let Some(search) = search {
        let search = search.to_ascii_lowercase();
        authors.retain(|author| author.name.to_ascii_lowercase().contains(&search));
    }

    Json(authors_v2_page_payload(authors, page, size, unpaged)).into_response()
}

pub async fn genres(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_genres(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn tags(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_tags(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_tags(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_id = query_value(query, "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let collection_id = query_value(query, "collection_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_series_tags(
        database_file,
        library_id.as_deref(),
        collection_id.as_deref(),
    )
    .await
    {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn languages(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_languages(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn publishers(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_publishers(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn age_ratings(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_age_ratings(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn sharing_labels(headers: HeaderMap, uri: Uri, database_file: &FsPath) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_sharing_labels(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub async fn series_release_dates(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let library_id = query_value(uri.query().unwrap_or_default(), "library_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);

    match load_persisted_series_release_dates(database_file, library_id.as_deref()).await {
        Ok(values) => Json(json!(values)).into_response(),
        Err(error) => internal_error_response(error),
    }
}
