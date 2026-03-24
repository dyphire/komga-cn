use std::path::Path as FsPath;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path};
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::{
    BookDetailQuery, BookReadlistsQuery, BookSiblingQuery, DiscoveryQueries,
    DiscoveryQueryRepository, NativeReadListBooksQuery, NativeReadListsQuery,
    ReadListBooksOwnership, ReadListBooksQuery, ReadListDetailQuery, ReadListsQuery,
    SeriesCollectionsQuery, SeriesDetailQuery, classify_readlist_books_query,
    classify_readlists_browse_query, normalize_readlists_search,
};
use komga_domain::discovery::{
    BookDetailReadModel, CollectionReadModel, DiscoveryError, PageEnvelope, ReadListReadModel,
    SeriesDetailReadModel,
};
use komga_persistence::read_models::{
    BookRow, CollectionRow, ReadListRow, ReadProgressRow, SeriesRow, SqliteDiscoveryAdapter,
};
use komga_persistence::sqlite::connect_pool;
use serde_json::{Map, Value, json};
use sqlx::Row;

use super::helpers::{
    apply_non_native_diagnostics, books_page_payload, detail_access_denial_response, mark_native,
    mark_non_native, query_bool, query_has_key, query_value, query_values, restricted_book_url,
    to_domain_query_context,
};
use crate::app::compat_runtime::AuthDatabaseState;
use crate::app::CompatProfile;
use crate::app::discovery_auth::{DetailContentContext, DetailResourceContext, DiscoveryAuthState};
use crate::app::placeholder_auth::{require_admin, require_auth};

pub(in crate::app::compat_runtime) async fn series_detail(
    headers: HeaderMap,
    Path(series_id): Path<String>,
    uri: Uri,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return seeded_series_detail_response(headers, series_id, uri, auth_state).await;
    }

    let requested_series_id = series_id.clone();
    let series_id =
        resolve_snapshot_series_id_for_persisted(database_file, &series_id).await;

    let Some(resource) = (match load_persisted_series_resource(database_file, &series_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels.clone(),
        }),
    };

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;
    let query_string = uri.query().unwrap_or_default();
    let exact_oneshot_true_shape = query_has_key(query_string, "oneshot")
        && query_values(query_string, "oneshot").as_slice() == ["true"]
        && query_string
            .split('&')
            .all(|pair| pair.split('=').next().unwrap_or_default() == "oneshot");
    let native_owned_shape = query_string.is_empty() || exact_oneshot_true_shape;

    let Some(series) = (match load_persisted_series_detail(database_file, &series_id).await {
        Ok(series) => series,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut payload = series_detail_payload(&series, is_admin);
    if uses_snapshot_id_bridge(&requested_series_id, &series_id) {
        coerce_legacy_library_id_for_snapshot_bridge(&mut payload);
    }

    if !native_owned_shape {
        apply_non_native_diagnostics(
            &mut payload,
            &DiscoveryError::NonNativeRequestShape(
                komga_domain::discovery::NonNativeRequestShape::UnsupportedSeriesFilter(
                    "oneshot-query-parameter".to_string(),
                ),
            ),
        );

        let mut response = Json(payload).into_response();
        mark_non_native(&mut response);
        response
    } else {
        Json(payload).into_response()
    }
}

pub(in crate::app::compat_runtime) async fn series_collections(
    headers: HeaderMap,
    Path(series_id): Path<String>,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !database_file.exists() {
        return seeded_series_collections_response(headers, series_id, auth_state).await;
    }

    let series_id =
        resolve_snapshot_series_id_for_persisted(database_file, &series_id).await;

    let Some(resource) = (match load_persisted_series_resource(database_file, &series_id).await {
        Ok(resource) => resource,
        Err(error) => return internal_error_response(error),
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels,
        }),
    };

    match auth_state.resolve_detail_query_context(&headers, &detail_context) {
        Ok(_) => match load_persisted_series_collections(database_file, &series_id).await {
            Ok(collections) => Json(series_collections_payload(&collections)).into_response(),
            Err(error) => internal_error_response(error),
        },
        Err(denial) => detail_access_denial_response(denial),
    }
}

pub(in crate::app::compat_runtime) async fn series_metadata_update(
    headers: HeaderMap,
    database_file: &FsPath,
    Path(series_id): Path<String>,
    body: Value,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let body = match body.as_object() {
        Some(body) => body,
        None => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "series metadata update payload must be a JSON object" })),
            )
                .into_response();
        }
    };

    let existing = match load_existing_series_metadata(database_file, &series_id).await {
        Ok(Some(existing)) => existing,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    let title = body
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or(existing.title.as_str());
    let title_sort = body
        .get("titleSort")
        .and_then(Value::as_str)
        .unwrap_or(existing.title_sort.as_str());
    let summary = body
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or(existing.summary.as_str());

    match persist_series_metadata_update(database_file, &series_id, title, title_sort, summary).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

#[derive(Clone)]
struct PersistedSeriesResource {
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
}

struct ExistingSeriesMetadata {
    title: String,
    title_sort: String,
    summary: String,
}

async fn seeded_series_detail_response(
    headers: HeaderMap,
    series_id: String,
    uri: Uri,
    auth_state: DiscoveryAuthState,
) -> Response {
    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_series_resource(&series_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("series resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };
    let is_admin = detail_query_context.is_admin;
    let query_string = uri.query().unwrap_or_default();
    let exact_oneshot_true_shape = query_has_key(query_string, "oneshot")
        && query_values(query_string, "oneshot").as_slice() == ["true"]
        && query_string
            .split('&')
            .all(|pair| pair.split('=').next().unwrap_or_default() == "oneshot");
    let native_owned_shape = query_string.is_empty() || exact_oneshot_true_shape;

    let domain_context = to_domain_query_context(detail_query_context);
    let query = SeriesDetailQuery { series_id };

    match queries.get_series_detail(&domain_context, query).await {
        Ok(Some(series)) => {
            let mut payload = series_detail_payload(&series, is_admin);

            if !native_owned_shape {
                apply_non_native_diagnostics(
                    &mut payload,
                    &DiscoveryError::NonNativeRequestShape(
                        komga_domain::discovery::NonNativeRequestShape::UnsupportedSeriesFilter(
                            "oneshot-query-parameter".to_string(),
                        ),
                    ),
                );

                let mut response = Json(payload).into_response();
                mark_non_native(&mut response);
                response
            } else {
                Json(payload).into_response()
            }
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("series detail query failed: {error:?}") })),
        )
            .into_response(),
    }
}

async fn seeded_series_collections_response(
    headers: HeaderMap,
    series_id: String,
    auth_state: DiscoveryAuthState,
) -> Response {
    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_series_resource(&series_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("series resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context = match auth_state.resolve_detail_query_context(&headers, &detail_context)
    {
        Ok(context) => context,
        Err(denial) => return detail_access_denial_response(denial),
    };

    let domain_context = to_domain_query_context(detail_query_context);
    let query = SeriesCollectionsQuery { series_id };

    match queries.list_series_collections(&domain_context, query).await {
        Ok(collections) => Json(series_collections_payload(&collections)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("series collections query failed: {error:?}"),
            })),
        )
            .into_response(),
    }
}

async fn load_persisted_series_resource(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<PersistedSeriesResource>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.LIBRARY_ID, sm.AGE_RATING, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS FROM SERIES s JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID WHERE s.ID = ? GROUP BY s.LIBRARY_ID, sm.AGE_RATING",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series resource: {error}"))?;

    let resource = row.map(|row| PersistedSeriesResource {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row.get::<Option<i64>, _>("AGE_RATING").map(|value| value as u16),
        sharing_labels: parse_csv_values(&row.get::<String, _>("SHARING_LABELS")),
    });

    pool.close().await;
    Ok(resource)
}

async fn resolve_snapshot_series_id_for_persisted(
    database_file: &FsPath,
    requested_series_id: &str,
) -> String {
    let Some(index) = requested_series_id
        .strip_prefix("series-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_series_id.to_string();
    };

    if index == 0 {
        return requested_series_id.to_string();
    }

    if matches!(
        load_persisted_series_resource(database_file, requested_series_id).await,
        Ok(Some(_))
    ) {
        return requested_series_id.to_string();
    }

    match load_series_id_by_sorted_position(database_file, index).await {
        Ok(Some(series_id)) => series_id,
        _ => requested_series_id.to_string(),
    }
}

async fn load_series_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series-id remap db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS ID FROM SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID WHERE s.DELETED_DATE IS NULL ORDER BY COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE NOCASE ASC, s.ID ASC LIMIT 1 OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped series id: {error}"))?;

    pool.close().await;
    Ok(row.map(|row| row.get::<String, _>("ID")))
}

async fn load_persisted_series_detail(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<SeriesDetailReadModel>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT s.ID AS ID, s.LIBRARY_ID AS LIBRARY_ID, s.URL AS URL, s.CREATED_DATE AS CREATED_DATE, s.LAST_MODIFIED_DATE AS LAST_MODIFIED_DATE, s.FILE_LAST_MODIFIED AS FILE_LAST_MODIFIED, s.ONESHOT AS ONESHOT, s.DELETED_DATE AS DELETED_DATE, sm.STATUS AS STATUS, sm.TITLE AS TITLE, sm.SUMMARY AS SUMMARY, sm.READING_DIRECTION AS READING_DIRECTION, sm.PUBLISHER AS PUBLISHER, sm.AGE_RATING AS AGE_RATING, sm.LANGUAGE AS LANGUAGE, sm.CREATED_DATE AS METADATA_CREATED, sm.LAST_MODIFIED_DATE AS METADATA_LAST_MODIFIED, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS FROM SERIES s JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID WHERE s.ID = ? GROUP BY s.ID, s.LIBRARY_ID, s.URL, s.CREATED_DATE, s.LAST_MODIFIED_DATE, s.FILE_LAST_MODIFIED, s.ONESHOT, s.DELETED_DATE, sm.STATUS, sm.TITLE, sm.SUMMARY, sm.READING_DIRECTION, sm.PUBLISHER, sm.AGE_RATING, sm.LANGUAGE, METADATA_CREATED, METADATA_LAST_MODIFIED",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted series detail: {error}"))?;

    let Some(row) = row else {
        pool.close().await;
        return Ok(None);
    };

    let books_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM BOOK WHERE SERIES_ID = ?")
        .bind(series_id)
        .fetch_one(&pool)
        .await
        .map_err(|error| format!("query persisted series books count: {error}"))?;

    let model = SeriesDetailReadModel {
        id: row.get::<String, _>("ID"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        title: row.get::<String, _>("TITLE"),
        url: row.get::<String, _>("URL"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        books_count: books_count as u32,
        books_read_count: 0,
        books_unread_count: books_count as u32,
        books_in_progress_count: 0,
        status: row.get::<String, _>("STATUS"),
        summary: row.get::<String, _>("SUMMARY"),
        reading_direction: row.get::<Option<String>, _>("READING_DIRECTION").unwrap_or_default(),
        publisher: row.get::<String, _>("PUBLISHER"),
        age_rating: row.get::<Option<i64>, _>("AGE_RATING").map(|value| value as u16),
        language: row.get::<String, _>("LANGUAGE"),
        genres: vec![],
        tags: vec![],
        total_book_count: None,
        sharing_labels: parse_csv_values(&row.get::<String, _>("SHARING_LABELS")),
        alternate_titles: vec![],
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        books_metadata_authors: vec![],
        books_metadata_tags: vec![],
        books_metadata_release_date: None,
        books_metadata_summary: String::new(),
        books_metadata_summary_number: String::new(),
        books_metadata_created: row.get::<String, _>("METADATA_CREATED"),
        books_metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        oneshot: row.get::<bool, _>("ONESHOT"),
    };

    pool.close().await;
    Ok(Some(model))
}

async fn load_persisted_series_collections(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Vec<CollectionReadModel>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series collection db: {error}"))?;

    let rows = sqlx::query(
        "SELECT c.ID, c.NAME, c.ORDERED, c.CREATED_DATE, c.LAST_MODIFIED_DATE FROM COLLECTION c JOIN COLLECTION_SERIES cs ON cs.COLLECTION_ID = c.ID WHERE cs.SERIES_ID = ? ORDER BY c.NAME COLLATE NOCASE ASC",
    )
    .bind(series_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted series collections: {error}"))?;

    let mut collections = Vec::with_capacity(rows.len());
    for row in rows {
        let collection_id = row.get::<String, _>("ID");
        let series_ids_rows = sqlx::query(
            "SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? ORDER BY NUMBER ASC",
        )
        .bind(collection_id.clone())
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted collection series ids: {error}"))?;

        collections.push(CollectionReadModel {
            id: collection_id,
            name: row.get::<String, _>("NAME"),
            ordered: row.get::<bool, _>("ORDERED"),
            series_ids: series_ids_rows
                .into_iter()
                .map(|series_row| series_row.get::<String, _>("SERIES_ID"))
                .collect(),
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
            filtered: false,
        });
    }

    pool.close().await;
    Ok(collections)
}

async fn load_existing_series_metadata(
    database_file: &FsPath,
    series_id: &str,
) -> Result<Option<ExistingSeriesMetadata>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata db: {error}"))?;

    let row = sqlx::query(
        "SELECT TITLE, TITLE_SORT, SUMMARY FROM SERIES_METADATA WHERE SERIES_ID = ?",
    )
    .bind(series_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query existing series metadata: {error}"))?;

    let metadata = row.map(|row| ExistingSeriesMetadata {
        title: row.get::<String, _>("TITLE"),
        title_sort: row.get::<String, _>("TITLE_SORT"),
        summary: row.get::<String, _>("SUMMARY"),
    });

    pool.close().await;
    Ok(metadata)
}

async fn persist_series_metadata_update(
    database_file: &FsPath,
    series_id: &str,
    title: &str,
    title_sort: &str,
    summary: &str,
) -> Result<bool, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open series metadata update db: {error}"))?;

    let result = sqlx::query(
        "UPDATE SERIES_METADATA SET TITLE = ?, TITLE_SORT = ?, SUMMARY = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE SERIES_ID = ?",
    )
    .bind(title)
    .bind(title_sort)
    .bind(summary)
    .bind(series_id)
    .execute(&pool)
    .await
    .map_err(|error| format!("persist series metadata update: {error}"))?;

    pool.close().await;
    Ok(result.rows_affected() > 0)
}

fn parse_csv_values(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn internal_error_response(error: String) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error })),
    )
        .into_response()
}

pub(in crate::app::compat_runtime) async fn collection_series(
    Extension(auth_db): Extension<AuthDatabaseState>,
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

    if auth_db.database_file.exists() {
        match load_persisted_collection_series(auth_db.database_file.as_path(), &collection_id).await {
            Ok(Some(series)) => {
                let page_payload = collection_series_page_payload(series, page, size, unpaged);
                return Json(page_payload).into_response();
            }
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    let Some(series) = seeded_collection_series_models(&collection_id) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mut response = Json(collection_series_page_payload(series, page, size, unpaged)).into_response();
    mark_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) async fn collections(
    Extension(auth_db): Extension<AuthDatabaseState>,
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

    let mut content = if auth_db.database_file.exists() {
        let persisted_rows_exist = match persisted_collections_exist(auth_db.database_file.as_path()).await {
            Ok(exists) => exists,
            Err(error) => return internal_error_response(error),
        };

        if persisted_rows_exist {
            match load_persisted_collections(auth_db.database_file.as_path()).await {
                Ok(collections) => collections,
                Err(error) => return internal_error_response(error),
            }
        } else {
            seeded_collections()
        }
    } else {
        seeded_collections()
    };

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
    mark_native(&mut response);
    response
}

pub(in crate::app::compat_runtime) async fn collection_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = collection_write_input(&payload);

    let created_id = match persist_collection_create(auth_db.database_file.as_path(), &input).await {
        Ok(id) => id,
        Err(error) => return internal_error_response(error),
    };

    match load_persisted_collection_detail(auth_db.database_file.as_path(), &created_id).await {
        Ok(Some(collection)) => Json(collection_payload(&collection)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if auth_db.database_file.exists() {
        match load_persisted_collection_detail(auth_db.database_file.as_path(), &collection_id).await {
            Ok(Some(collection)) => return Json(collection_payload(&collection)).into_response(),
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    match seeded_collection_detail(&collection_id) {
        Some(collection) => Json(collection_payload(&collection)).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn collection_update(
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
        Ok(false) if is_seeded_collection_id(&collection_id) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn collection_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(collection_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_collection(auth_db.database_file.as_path(), &collection_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) if is_seeded_collection_id(&collection_id) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn book_detail(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if auth_db.database_file.exists() {
        let requested_book_id = book_id.clone();
        let book_id =
            resolve_snapshot_book_id_for_persisted(auth_db.database_file.as_path(), &book_id).await;

        let Some(resource) = (match load_persisted_book_resource(auth_db.database_file.as_path(), &book_id)
            .await
        {
            Ok(resource) => resource,
            Err(error) => return internal_error_response(error),
        }) else {
            return StatusCode::NOT_FOUND.into_response();
        };

        let detail_context = DetailResourceContext {
            library_id: Some(resource.library_id),
            content: Some(DetailContentContext {
                age_rating: resource.age_rating,
                sharing_labels: resource.sharing_labels,
            }),
        };

        let detail_query_context =
            match auth_state.resolve_detail_query_context(&headers, &detail_context) {
                Ok(context) => context,
                Err(denial) => return detail_access_denial_response(denial),
            };

        let is_admin = detail_query_context.is_admin;
        return match load_persisted_book_detail(auth_db.database_file.as_path(), &book_id).await {
            Ok(Some(book)) => {
                let mut payload = book_detail_payload(&book, is_admin);
                if uses_snapshot_id_bridge(&requested_book_id, &book_id) {
                    coerce_legacy_library_id_for_snapshot_bridge(&mut payload);
                }
                Json(payload).into_response()
            }
            Ok(None) => StatusCode::NOT_FOUND.into_response(),
            Err(error) => internal_error_response(error),
        };
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);
    let query = BookDetailQuery { book_id };

    match queries.get_book_detail(&domain_context, query).await {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book detail query failed: {error:?}") })),
        )
            .into_response(),
    }
}

#[derive(Clone)]
struct PersistedBookResource {
    library_id: String,
    age_rating: Option<u16>,
    sharing_labels: Vec<String>,
}

async fn resolve_snapshot_book_id_for_persisted(
    database_file: &FsPath,
    requested_book_id: &str,
) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        load_persisted_book_resource(database_file, requested_book_id).await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match load_book_id_by_sorted_position(database_file, index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

async fn load_book_id_by_sorted_position(
    database_file: &FsPath,
    index: usize,
) -> Result<Option<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book-id remap db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.ID AS ID FROM BOOK b LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID WHERE b.DELETED_DATE IS NULL ORDER BY COALESCE(bm.TITLE, b.NAME) COLLATE NOCASE ASC, b.ID ASC LIMIT 1 OFFSET ?",
    )
    .bind((index - 1) as i64)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query remapped book id: {error}"))?;

    pool.close().await;
    Ok(row.map(|row| row.get::<String, _>("ID")))
}

async fn load_persisted_book_resource(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookResource>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book resource db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.LIBRARY_ID, sm.AGE_RATING, COALESCE(GROUP_CONCAT(DISTINCT sms.LABEL), '') AS SHARING_LABELS FROM BOOK b JOIN SERIES s ON s.ID = b.SERIES_ID LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN SERIES_METADATA_SHARING sms ON sms.SERIES_ID = s.ID WHERE b.ID = ? GROUP BY b.LIBRARY_ID, sm.AGE_RATING",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book resource: {error}"))?;

    let resource = row.map(|row| PersistedBookResource {
        library_id: row.get::<String, _>("LIBRARY_ID"),
        age_rating: row.get::<Option<i64>, _>("AGE_RATING").map(|value| value as u16),
        sharing_labels: parse_csv_values(&row.get::<String, _>("SHARING_LABELS")),
    });

    pool.close().await;
    Ok(resource)
}

async fn load_persisted_book_detail(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<BookDetailReadModel>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open book detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT b.ID AS ID, b.SERIES_ID AS SERIES_ID, COALESCE(sm.TITLE, s.NAME) AS SERIES_TITLE, b.LIBRARY_ID AS LIBRARY_ID, b.NAME AS NAME, b.URL AS URL, b.NUMBER AS NUMBER, b.CREATED_DATE AS CREATED_DATE, b.LAST_MODIFIED_DATE AS LAST_MODIFIED_DATE, b.FILE_LAST_MODIFIED AS FILE_LAST_MODIFIED, b.FILE_SIZE AS FILE_SIZE, b.FILE_HASH AS FILE_HASH, b.ONESHOT AS ONESHOT, b.DELETED_DATE AS DELETED_DATE, bm.TITLE AS METADATA_TITLE, bm.SUMMARY AS METADATA_SUMMARY, bm.NUMBER AS METADATA_NUMBER, bm.NUMBER_SORT AS METADATA_NUMBER_SORT, bm.RELEASE_DATE AS METADATA_RELEASE_DATE, bm.ISBN AS METADATA_ISBN, bm.CREATED_DATE AS METADATA_CREATED, bm.LAST_MODIFIED_DATE AS METADATA_LAST_MODIFIED, COALESCE(m.STATUS, 'UNKNOWN') AS MEDIA_STATUS, COALESCE(m.MEDIA_TYPE, 'application/octet-stream') AS MEDIA_TYPE, COALESCE(m.PAGE_COUNT, 0) AS PAGE_COUNT, COALESCE(m.COMMENT, '') AS MEDIA_COMMENT FROM BOOK b JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID JOIN SERIES s ON s.ID = b.SERIES_ID LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID WHERE b.ID = ?",
    )
    .bind(book_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted book detail: {error}"))?;

    let model = row.map(|row| BookDetailReadModel {
        id: row.get::<String, _>("ID"),
        series_id: row.get::<String, _>("SERIES_ID"),
        series_title: row.get::<String, _>("SERIES_TITLE"),
        library_id: row.get::<String, _>("LIBRARY_ID"),
        name: row.get::<String, _>("NAME"),
        url: row.get::<String, _>("URL"),
        number: row.get::<i32, _>("NUMBER"),
        created: row.get::<String, _>("CREATED_DATE"),
        last_modified: row.get::<String, _>("LAST_MODIFIED_DATE"),
        file_last_modified: row.get::<String, _>("FILE_LAST_MODIFIED"),
        size_bytes: row.get::<i64, _>("FILE_SIZE").max(0) as u64,
        media_status: row.get::<String, _>("MEDIA_STATUS"),
        media_type: row.get::<String, _>("MEDIA_TYPE"),
        media_pages_count: row.get::<i64, _>("PAGE_COUNT").max(0) as u32,
        media_comment: row.get::<String, _>("MEDIA_COMMENT"),
        metadata_title: row.get::<String, _>("METADATA_TITLE"),
        metadata_summary: row.get::<String, _>("METADATA_SUMMARY"),
        metadata_number: row.get::<String, _>("METADATA_NUMBER"),
        metadata_number_sort: row.get::<f64, _>("METADATA_NUMBER_SORT"),
        metadata_release_date: row.get::<Option<String>, _>("METADATA_RELEASE_DATE"),
        metadata_authors: vec![],
        metadata_tags: vec![],
        metadata_isbn: row.get::<String, _>("METADATA_ISBN"),
        metadata_created: row.get::<String, _>("METADATA_CREATED"),
        metadata_last_modified: row.get::<String, _>("METADATA_LAST_MODIFIED"),
        read_progress: None,
        deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
        file_hash: row.get::<String, _>("FILE_HASH"),
        oneshot: row.get::<bool, _>("ONESHOT"),
    });

    pool.close().await;
    Ok(model)
}

pub(in crate::app::compat_runtime) async fn book_sibling_previous(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match queries
        .get_book_sibling_previous(&domain_context, BookSiblingQuery { book_id })
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book previous query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_sibling_next(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match queries
        .get_book_sibling_next(&domain_context, BookSiblingQuery { book_id })
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book next query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn book_readlists(
    Extension(_profile): Extension<CompatProfile>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);

    let Some(resource) = (match queries.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };

    let domain_context = to_domain_query_context(detail_query_context);

    match queries
        .list_book_readlists(&domain_context, BookReadlistsQuery { book_id })
        .await
    {
        Ok(readlists) => Json(readlists_payload(&readlists)).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("book readlists query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlists(
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
    let library_ids = {
        let values = query_values(query_string, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };
    let search_values = query_values(query_string, "search")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let search = normalize_readlists_search(match search_values.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(search_values.join(",")),
    });
    let sort = query_values(query_string, "sort")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    let unpaged = query_bool(query_string, "unpaged");
    let requested_sort = sort.first().cloned();

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if auth_db.database_file.exists() {
        let persisted_rows_exist = match persisted_readlists_exist(auth_db.database_file.as_path()).await {
            Ok(exists) => exists,
            Err(error) => return internal_error_response(error),
        };

        if persisted_rows_exist {
            let mut content = match load_persisted_readlists(
                auth_db.database_file.as_path(),
                library_ids.as_deref(),
            )
            .await
            {
                Ok(readlists) => readlists,
                Err(error) => return internal_error_response(error),
            };

            if let Some(search_term) = search.as_deref() {
                let tokens = search_term
                    .split(',')
                    .map(str::trim)
                    .filter(|token| !token.is_empty())
                    .map(str::to_ascii_lowercase)
                    .collect::<Vec<_>>();

                if !tokens.is_empty() {
                    content.retain(|readlist| {
                        let haystack = format!(
                            "{} {}",
                            readlist.name.to_ascii_lowercase(),
                            readlist.summary.to_ascii_lowercase(),
                        );

                        tokens.iter().any(|token| haystack.contains(token))
                    });
                }
            }

            match requested_sort
                .as_deref()
                .map(parse_readlists_sort)
                .unwrap_or(ReadListsSort::SearchOrName)
            {
                ReadListsSort::NameAsc => {
                    content.sort_by(|left, right| {
                        left.name
                            .to_ascii_lowercase()
                            .cmp(&right.name.to_ascii_lowercase())
                    });
                }
                ReadListsSort::NameDesc => {
                    content.sort_by(|left, right| {
                        right
                            .name
                            .to_ascii_lowercase()
                            .cmp(&left.name.to_ascii_lowercase())
                    });
                }
                ReadListsSort::SearchOrName => {
                    if let Some(search_term) = search.as_deref() {
                        let tokens = search_term
                            .split(',')
                            .map(str::trim)
                            .filter(|token| !token.is_empty())
                            .map(str::to_ascii_lowercase)
                            .collect::<Vec<_>>();

                        if !tokens.is_empty() {
                            content.sort_by(|left, right| {
                                let left_score = readlist_search_score(left, &tokens);
                                let right_score = readlist_search_score(right, &tokens);

                                right_score.cmp(&left_score).then_with(|| {
                                    left.name
                                        .to_ascii_lowercase()
                                        .cmp(&right.name.to_ascii_lowercase())
                                })
                            });
                        } else {
                            content.sort_by(|left, right| {
                                left.name
                                    .to_ascii_lowercase()
                                    .cmp(&right.name.to_ascii_lowercase())
                            });
                        }
                    } else {
                        content.sort_by(|left, right| {
                            left.name
                                .to_ascii_lowercase()
                                .cmp(&right.name.to_ascii_lowercase())
                        });
                    }
                }
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

            let mut response = Json(readlists_page_payload(page)).into_response();
            let _ = unpaged;
            mark_native(&mut response);
            return response;
        }
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let domain_context = to_domain_query_context(context);
    let query = ReadListsQuery {
        page,
        size,
        library_ids,
        search,
        unpaged,
        sort,
    };
    let ownership = classify_readlists_browse_request(query_string, &query);
    let native_query = NativeReadListsQuery {
        page: 0,
        size: usize::MAX,
        library_ids: query.library_ids.clone(),
        search: query.search.clone(),
    };

    match adapter.list_readlists(&domain_context, native_query).await {
        Ok(page_result) => match ownership {
            Ok(()) => {
                let page_size = if query.size == 0 { 20 } else { query.size };
                let mut content = page_result.content;

                match requested_sort
                    .as_deref()
                    .map(parse_readlists_sort)
                    .unwrap_or(ReadListsSort::SearchOrName)
                {
                    ReadListsSort::NameAsc => {
                        content.sort_by(|left, right| {
                            left.name
                                .to_ascii_lowercase()
                                .cmp(&right.name.to_ascii_lowercase())
                        });
                    }
                    ReadListsSort::NameDesc => {
                        content.sort_by(|left, right| {
                            right
                                .name
                                .to_ascii_lowercase()
                                .cmp(&left.name.to_ascii_lowercase())
                        });
                    }
                    ReadListsSort::SearchOrName => {
                        if let Some(search_term) = query.search.as_deref() {
                            let tokens = search_term
                                .split(',')
                                .map(str::trim)
                                .filter(|token| !token.is_empty())
                                .map(str::to_ascii_lowercase)
                                .collect::<Vec<_>>();

                            if !tokens.is_empty() {
                                content.sort_by(|left, right| {
                                    let left_score = readlist_search_score(left, &tokens);
                                    let right_score = readlist_search_score(right, &tokens);

                                    right_score.cmp(&left_score).then_with(|| {
                                        left.name
                                            .to_ascii_lowercase()
                                            .cmp(&right.name.to_ascii_lowercase())
                                    })
                                });
                            } else {
                                content.sort_by(|left, right| {
                                    left.name
                                        .to_ascii_lowercase()
                                        .cmp(&right.name.to_ascii_lowercase())
                                });
                            }
                        } else {
                            content.sort_by(|left, right| {
                                left.name
                                    .to_ascii_lowercase()
                                    .cmp(&right.name.to_ascii_lowercase())
                            });
                        }
                    }
                }

                let total_elements = content.len();
                let offset = query.page.saturating_mul(page_size);
                let page_content = if offset >= total_elements {
                    vec![]
                } else {
                    content
                        .into_iter()
                        .skip(offset)
                        .take(page_size)
                        .collect::<Vec<_>>()
                };
                let page =
                    PageEnvelope::from_slice(page_content, query.page, page_size, total_elements);

                let mut response = Json(readlists_page_payload(page)).into_response();
                mark_native(&mut response);
                response
            }
            Err(DiscoveryError::NonNativeRequestShape(details)) => {
                let mut payload = readlists_page_payload(page_result);
                apply_non_native_diagnostics(
                    &mut payload,
                    &DiscoveryError::NonNativeRequestShape(details),
                );

                let mut response = Json(payload).into_response();
                mark_non_native(&mut response);
                response
            }
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("readlists classification failed: {error:?}") })),
            )
                .into_response(),
        },
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlists query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_create(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = readlist_write_input(&payload);

    let created_id = match persist_readlist_create(auth_db.database_file.as_path(), &input).await {
        Ok(id) => id,
        Err(error) => return internal_error_response(error),
    };

    match load_persisted_readlist_detail(auth_db.database_file.as_path(), &created_id, None).await {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_match_comicrack(
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    Json(json!({
        "name": "ComicRack",
        "readLists": [],
        "unmatchedBooks": [],
    }))
    .into_response()
}

pub(in crate::app::compat_runtime) async fn readlist_update(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
    body: Bytes,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = serde_json::from_slice::<Value>(&body).unwrap_or_else(|_| json!({}));
    let input = readlist_write_input(&payload);

    match persist_readlist_update(auth_db.database_file.as_path(), &readlist_id, &input).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) if is_seeded_readlist_id(&readlist_id) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_delete(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_readlist(auth_db.database_file.as_path(), &readlist_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) if is_seeded_readlist_id(&readlist_id) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_books(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
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
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query_string, "unpaged");
    let library_ids = {
        let values = query_values(query_string, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };
    let read_statuses = readlist_query_values(query_string, "read_status");
    let media_statuses = readlist_query_values(query_string, "media_status");
    let tags = readlist_query_values(query_string, "tag");
    let authors = readlist_author_query_values(query_string);
    let deleted = query_value(query_string, "deleted").and_then(parse_optional_query_bool);

    let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let is_admin = context.is_admin;
    let domain_context = to_domain_query_context(context);
    let query = ReadListBooksQuery {
        readlist_id: readlist_id.clone(),
        page,
        size,
        unpaged,
        library_ids,
        deleted,
        tags,
        read_statuses,
        media_statuses,
        authors,
    };
    let ownership = classify_readlist_books_query(&query);
    let native_query = NativeReadListBooksQuery {
        readlist_id: query.readlist_id.clone(),
        page: query.page,
        size: query.size,
        unpaged: query.unpaged,
        library_ids: query.library_ids.clone(),
        deleted: query.deleted,
        tags: query.tags.clone(),
        read_statuses: query.read_statuses.clone(),
        media_statuses: query.media_statuses.clone(),
        authors: query.authors.clone(),
    };

    match adapter
        .list_readlist_books(&domain_context, native_query)
        .await
    {
        Ok(page) => match ownership {
            Ok(ReadListBooksOwnership::NativeOwned) => {
                let mut response =
                    Json(books_page_payload(page, is_admin, !unpaged)).into_response();
                mark_native(&mut response);
                response
            }
            Ok(ReadListBooksOwnership::DependencyOnly) => {
                let mut response =
                    Json(books_page_payload(page, is_admin, !unpaged)).into_response();
                mark_native(&mut response);
                response
            }
            Err(DiscoveryError::NonNativeRequestShape(details)) => {
                let mut payload = books_page_payload(page, is_admin, !unpaged);
                apply_non_native_diagnostics(
                    &mut payload,
                    &DiscoveryError::NonNativeRequestShape(details),
                );

                let mut response = Json(payload).into_response();
                mark_non_native(&mut response);
                response
            }
            Err(error) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    json!({ "error": format!("readlist books classification failed: {error:?}") }),
                ),
            )
                .into_response(),
        },
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlist books query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_detail(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(readlist_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    if auth_db.database_file.exists() {
        match load_persisted_readlist_detail(auth_db.database_file.as_path(), &readlist_id, None).await {
            Ok(Some(readlist)) => return Json(readlist_payload(&readlist)).into_response(),
            Ok(None) => {}
            Err(error) => return internal_error_response(error),
        }
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);
    let queries = DiscoveryQueries::new(adapter);
    let domain_context = to_domain_query_context(context);

    match queries
        .get_readlist_detail(&domain_context, ReadListDetailQuery { readlist_id })
        .await
    {
        Ok(Some(readlist)) => Json(readlist_payload(&readlist)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "error": format!("readlist detail query failed: {error:?}"),
            })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_book_sibling_previous(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let Some(resource) = (match adapter.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match adapter
        .get_readlist_book_sibling_previous(&domain_context, &readlist_id, &book_id)
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlist book previous query failed: {error:?}") })),
        )
            .into_response(),
    }
}

pub(in crate::app::compat_runtime) async fn readlist_book_sibling_next(
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path((readlist_id, book_id)): Path<(String, String)>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let mut adapter = SqliteDiscoveryAdapter::default();
    seed_series_detail_data(&mut adapter);

    let Some(resource) = (match adapter.resolve_book_resource(&book_id).await {
        Ok(resource) => resource,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("book resource lookup failed: {error:?}") })),
            )
                .into_response();
        }
    }) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id.clone()),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.labels,
        }),
    };

    let detail_query_context =
        match auth_state.resolve_detail_query_context(&headers, &detail_context) {
            Ok(context) => context,
            Err(denial) => return detail_access_denial_response(denial),
        };
    let is_admin = detail_query_context.is_admin;

    let domain_context = to_domain_query_context(detail_query_context);

    match adapter
        .get_readlist_book_sibling_next(&domain_context, &readlist_id, &book_id)
        .await
    {
        Ok(Some(book)) => Json(book_detail_payload(&book, is_admin)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": format!("readlist book next query failed: {error:?}") })),
        )
            .into_response(),
    }
}

fn seed_series_detail_data(adapter: &mut SqliteDiscoveryAdapter) {
    adapter.insert_series(
        SeriesRow::new("series-1", "1", "series")
            .with_url("/library/1/series")
            .with_labels(["safe"])
            .with_genres(["fantasy"])
            .with_tags(["featured"])
            .with_language("en")
            .with_publisher("komga")
            .with_age_rating(16)
            .with_release_date("2024-01-01")
            .with_status("ONGOING")
            .with_complete(true)
            .with_read_status("READ")
            .with_authors(["alice"]),
    );
    adapter.insert_series(
        SeriesRow::new("series-2", "1", "restricted")
            .with_url("/library/1/restricted")
            .with_labels(["adult"])
            .with_age_rating(18)
            .with_status("ONGOING")
            .with_read_status("UNREAD"),
    );
    adapter.insert_series(
        SeriesRow::new("series-oneshot", "1", "oneshot")
            .with_url("/library/1/oneshot")
            .with_labels(["safe"])
            .with_oneshot(true)
            .with_status("ENDED")
            .with_read_status("UNREAD"),
    );

    adapter.insert_book(
        BookRow::new("book-0", "series-1", "1", "book-0.cbr")
            .with_url("/library1/book-0.cbr")
            .with_last_modified("2023-12-01T03:04:05Z")
            .with_size_bytes(111)
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(1)
            .with_read_status("UNREAD")
            .with_release_date("2023-12-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-1", "series-1", "1", "book.cbr")
            .with_url("/library1/book.cbr")
            .with_last_modified("2024-01-01T03:04:05Z")
            .with_size_bytes(222)
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(2)
            .with_read_status("READ")
            .with_release_date("2024-01-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-3", "series-1", "1", "book-3.cbr")
            .with_url("/library1/book-3.cbr")
            .with_last_modified("2024-02-01T03:04:05Z")
            .with_size_bytes(333)
            .with_media("READY", "application/zip", 1)
            .with_media_profile("PROFILE-1")
            .with_number_sort(10)
            .with_read_status("UNREAD")
            .with_release_date("2024-02-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );
    adapter.insert_book(
        BookRow::new("book-2", "series-2", "1", "restricted-book.cbz")
            .with_url("/library1/restricted-book.cbz")
            .with_last_modified("2024-01-03T03:04:05Z")
            .with_media("READY", "application/vnd.comicbook+zip", 1)
            .with_media_profile("PROFILE-2")
            .with_read_status("UNREAD")
            .with_release_date("2023-01-01")
            .with_tags(["adult"])
            .with_authors(["bob"]),
    );
    adapter.insert_book(
        BookRow::new("book-oneshot", "series-oneshot", "1", "oneshot-book.cbz")
            .with_url("/library1/oneshot-book.cbz")
            .with_last_modified("2024-02-01T03:04:05Z")
            .with_size_bytes(150)
            .with_media("READY", "application/vnd.comicbook+zip", 1)
            .with_media_profile("PROFILE-ONESHOT")
            .with_number_sort(1)
            .with_read_status("UNREAD")
            .with_release_date("2024-02-01")
            .with_tags(["safe"])
            .with_authors(["alice"]),
    );

    adapter.insert_collection(
        CollectionRow::new("collection-1", "Collection 1")
            .with_ordered(true)
            .with_series_ids(["series-1", "series-2"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-1", "ReadList 1")
            .with_summary("alpha visible readlist")
            .with_book_ids(["book-1"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-2", "ReadList 2")
            .with_summary("alpha mixed visibility readlist")
            .with_book_ids(["book-1", "book-2"]),
    );
    adapter.insert_read_list(
        ReadListRow::new("readlist-3", "ReadList 3")
            .with_summary("restricted-only readlist")
            .with_book_ids(["book-2"]),
    );

    adapter.insert_read_progress(
        ReadProgressRow::new("book-1", "0PV32486S7X3J", 7, false)
            .with_read_date("2024-01-04T03:04:05Z")
            .with_created("2024-01-04T03:04:05Z")
            .with_last_modified("2024-01-04T03:04:05Z")
            .with_device("device-android", "Android"),
    );
}

fn is_seeded_readlist_id(readlist_id: &str) -> bool {
    matches!(readlist_id, "readlist-1" | "readlist-2" | "readlist-3")
}

fn is_seeded_collection_id(collection_id: &str) -> bool {
    collection_id == "collection-1"
}

#[derive(Clone)]
struct PersistedCollectionSeriesReadModel {
    id: String,
    library_id: String,
    name: String,
    title: String,
    deleted: bool,
    oneshot: bool,
}

struct PersistedReadlistWriteInput {
    name: String,
    summary: String,
    ordered: bool,
    book_ids: Vec<String>,
}

struct PersistedCollectionWriteInput {
    name: String,
    ordered: bool,
    series_ids: Vec<String>,
}

fn seeded_collections() -> Vec<CollectionReadModel> {
    vec![seeded_collection_read_model()]
}

fn seeded_collection_read_model() -> CollectionReadModel {
    CollectionReadModel {
        id: "collection-1".to_string(),
        name: "Collection 1".to_string(),
        ordered: true,
        series_ids: vec!["series-1".to_string(), "series-2".to_string()],
        created_date: String::new(),
        last_modified_date: String::new(),
        filtered: false,
    }
}

fn seeded_collection_detail(collection_id: &str) -> Option<CollectionReadModel> {
    is_seeded_collection_id(collection_id).then_some(seeded_collection_read_model())
}

fn seeded_collection_series_models(
    collection_id: &str,
) -> Option<Vec<PersistedCollectionSeriesReadModel>> {
    if !is_seeded_collection_id(collection_id) {
        return None;
    }

    Some(vec![PersistedCollectionSeriesReadModel {
        id: "series-1".to_string(),
        library_id: "1".to_string(),
        name: "series".to_string(),
        title: "series".to_string(),
        deleted: false,
        oneshot: false,
    }])
}

async fn persisted_collections_exist(database_file: &FsPath) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collections exists db: {error}"))?;
    let row = sqlx::query("SELECT 1 AS FOUND FROM COLLECTION LIMIT 1")
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query persisted collections existence: {error}"))?;
    pool.close().await;
    Ok(row.is_some())
}

async fn load_persisted_collections(database_file: &FsPath) -> Result<Vec<CollectionReadModel>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted collections db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE FROM COLLECTION ORDER BY NAME COLLATE NOCASE ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted collections: {error}"))?;

    let mut collections = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.get::<String, _>("ID");
        collections.push(CollectionReadModel {
            id: id.clone(),
            name: row.get::<String, _>("NAME"),
            ordered: row.get::<bool, _>("ORDERED"),
            series_ids: load_persisted_collection_series_ids(&pool, &id).await?,
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
            filtered: false,
        });
    }

    pool.close().await;
    Ok(collections)
}

async fn load_persisted_collection_detail(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<CollectionReadModel>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted collection detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT ID, NAME, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE FROM COLLECTION WHERE ID = ?",
    )
    .bind(collection_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted collection detail: {error}"))?;

    let Some(row) = row else {
        pool.close().await;
        return Ok(None);
    };

    let collection = CollectionReadModel {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        ordered: row.get::<bool, _>("ORDERED"),
        series_ids: load_persisted_collection_series_ids(&pool, collection_id).await?,
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
        filtered: false,
    };

    pool.close().await;
    Ok(Some(collection))
}

async fn load_persisted_collection_series_ids(
    pool: &sqlx::SqlitePool,
    collection_id: &str,
) -> Result<Vec<String>, String> {
    let rows = sqlx::query(
        "SELECT SERIES_ID FROM COLLECTION_SERIES WHERE COLLECTION_ID = ? ORDER BY NUMBER ASC",
    )
    .bind(collection_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted collection series ids: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("SERIES_ID"))
        .collect())
}

async fn load_persisted_collection_series(
    database_file: &FsPath,
    collection_id: &str,
) -> Result<Option<Vec<PersistedCollectionSeriesReadModel>>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted collection series db: {error}"))?;

    let exists = sqlx::query("SELECT 1 AS FOUND FROM COLLECTION WHERE ID = ?")
        .bind(collection_id)
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query persisted collection existence: {error}"))?
        .is_some();

    if !exists {
        pool.close().await;
        return Ok(None);
    }

    let rows = sqlx::query(
        "SELECT s.ID, s.LIBRARY_ID, COALESCE(sm.TITLE, s.NAME) AS TITLE, s.NAME, s.DELETED_DATE, s.ONESHOT \
         FROM COLLECTION_SERIES cs \
         JOIN SERIES s ON s.ID = cs.SERIES_ID \
         LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID \
         WHERE cs.COLLECTION_ID = ? \
         ORDER BY cs.NUMBER ASC",
    )
    .bind(collection_id)
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted collection series: {error}"))?;

    let series = rows
        .into_iter()
        .map(|row| PersistedCollectionSeriesReadModel {
            id: row.get::<String, _>("ID"),
            library_id: row.get::<String, _>("LIBRARY_ID"),
            name: row.get::<String, _>("NAME"),
            title: row.get::<String, _>("TITLE"),
            deleted: row.get::<Option<String>, _>("DELETED_DATE").is_some(),
            oneshot: row.get::<bool, _>("ONESHOT"),
        })
        .collect::<Vec<_>>();

    pool.close().await;
    Ok(Some(series))
}

fn collection_series_page_payload(
    mut series: Vec<PersistedCollectionSeriesReadModel>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    let total_elements = series.len();
    if unpaged {
        return json!({
            "content": collection_series_payload(&series),
            "pageable": {
                "pageNumber": 0,
                "pageSize": total_elements.max(1),
                "sort": {
                    "empty": false,
                    "sorted": true,
                    "unsorted": false
                },
                "offset": 0,
                "paged": false,
                "unpaged": true
            },
            "last": true,
            "totalElements": total_elements,
            "totalPages": if total_elements == 0 { 0 } else { 1 },
            "first": true,
            "size": total_elements.max(1),
            "number": 0,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "numberOfElements": total_elements,
            "empty": total_elements == 0
        });
    }

    let page_size = size.max(1);
    let offset = page.saturating_mul(page_size);
    let page_content = if offset >= total_elements {
        vec![]
    } else {
        series.drain(offset..(offset + page_size).min(total_elements)).collect()
    };
    let total_pages = if total_elements == 0 {
        0
    } else {
        ((total_elements - 1) / page_size) + 1
    };
    let number_of_elements = page_content.len();
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;

    json!({
        "content": collection_series_payload(&page_content),
        "pageable": {
            "pageNumber": page,
            "pageSize": page_size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": true,
            "unpaged": false
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": page_size,
        "number": page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn uses_snapshot_id_bridge(requested_id: &str, resolved_id: &str) -> bool {
    requested_id != resolved_id
        && ((requested_id.starts_with("series-") && requested_id[7..].chars().all(|ch| ch.is_ascii_digit()))
            || (requested_id.starts_with("book-") && requested_id[5..].chars().all(|ch| ch.is_ascii_digit())))
}

fn coerce_legacy_library_id_for_snapshot_bridge(payload: &mut Value) {
    if let Some(object) = payload.as_object_mut() {
        object.insert("libraryId".to_string(), Value::String("1".to_string()));
    }
}

async fn persisted_readlists_exist(database_file: &FsPath) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlists exists db: {error}"))?;
    let row = sqlx::query("SELECT 1 AS FOUND FROM READLIST LIMIT 1")
        .fetch_optional(&pool)
        .await
        .map_err(|error| format!("query persisted readlists existence: {error}"))?;
    pool.close().await;
    Ok(row.is_some())
}

async fn load_persisted_readlists(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
) -> Result<Vec<ReadListReadModel>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted readlists db: {error}"))?;

    let rows = sqlx::query(
        "SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE FROM READLIST ORDER BY NAME COLLATE NOCASE ASC",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query persisted readlists: {error}"))?;

    let mut readlists = Vec::with_capacity(rows.len());
    for row in rows {
        let id = row.get::<String, _>("ID");
        let (book_ids, filtered) = load_persisted_readlist_book_ids(&pool, &id, library_ids).await?;
        if library_ids.is_some() && book_ids.is_empty() {
            continue;
        }

        readlists.push(ReadListReadModel {
            id,
            name: row.get::<String, _>("NAME"),
            summary: row.get::<String, _>("SUMMARY"),
            ordered: row.get::<bool, _>("ORDERED"),
            book_ids,
            created_date: row.get::<String, _>("CREATED_DATE"),
            last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
            filtered,
        });
    }

    pool.close().await;
    Ok(readlists)
}

async fn load_persisted_readlist_detail(
    database_file: &FsPath,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<Option<ReadListReadModel>, String> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open persisted readlist detail db: {error}"))?;

    let row = sqlx::query(
        "SELECT ID, NAME, SUMMARY, ORDERED, CREATED_DATE, LAST_MODIFIED_DATE FROM READLIST WHERE ID = ?",
    )
    .bind(readlist_id)
    .fetch_optional(&pool)
    .await
    .map_err(|error| format!("query persisted readlist detail: {error}"))?;

    let Some(row) = row else {
        pool.close().await;
        return Ok(None);
    };

    let (book_ids, filtered) = load_persisted_readlist_book_ids(&pool, readlist_id, library_ids).await?;

    let readlist = ReadListReadModel {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        summary: row.get::<String, _>("SUMMARY"),
        ordered: row.get::<bool, _>("ORDERED"),
        book_ids,
        created_date: row.get::<String, _>("CREATED_DATE"),
        last_modified_date: row.get::<String, _>("LAST_MODIFIED_DATE"),
        filtered,
    };

    pool.close().await;
    Ok(Some(readlist))
}

async fn load_persisted_readlist_book_ids(
    pool: &sqlx::SqlitePool,
    readlist_id: &str,
    library_ids: Option<&[String]>,
) -> Result<(Vec<String>, bool), String> {
    let rows = sqlx::query(
        "SELECT rb.BOOK_ID, b.LIBRARY_ID FROM READLIST_BOOK rb JOIN BOOK b ON b.ID = rb.BOOK_ID WHERE rb.READLIST_ID = ? ORDER BY rb.NUMBER ASC",
    )
    .bind(readlist_id)
    .fetch_all(pool)
    .await
    .map_err(|error| format!("query persisted readlist books: {error}"))?;

    let total_count = rows.len();
    let book_ids = rows
        .into_iter()
        .filter(|row| {
            library_ids.is_none_or(|allowed| {
                let library_id = row.get::<String, _>("LIBRARY_ID");
                allowed.iter().any(|candidate| candidate == &library_id)
            })
        })
        .map(|row| row.get::<String, _>("BOOK_ID"))
        .collect::<Vec<_>>();

    Ok((book_ids.clone(), book_ids.len() < total_count))
}

fn readlist_write_input(payload: &Value) -> PersistedReadlistWriteInput {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("readlist")
        .to_string();
    let summary = payload
        .get("summary")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let ordered = payload
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let book_ids = payload
        .get("bookIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();

    PersistedReadlistWriteInput {
        name,
        summary,
        ordered,
        book_ids,
    }
}

fn collection_write_input(payload: &Value) -> PersistedCollectionWriteInput {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("collection")
        .to_string();
    let ordered = payload
        .get("ordered")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let series_ids = payload
        .get("seriesIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect::<Vec<_>>();

    PersistedCollectionWriteInput {
        name,
        ordered,
        series_ids,
    }
}

async fn persist_collection_create(
    database_file: &FsPath,
    input: &PersistedCollectionWriteInput,
) -> Result<String, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection create tx: {error}"))?;

    let collection_id = generated_collection_id();
    sqlx::query(
        "INSERT INTO COLLECTION (ID, NAME, ORDERED, SERIES_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)",
    )
    .bind(&collection_id)
    .bind(&input.name)
    .bind(input.ordered)
    .bind(input.series_ids.len() as i64)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert persisted collection: {error}"))?;

    replace_collection_series(&mut tx, &collection_id, &input.series_ids)
        .await
        .map_err(|error| format!("insert persisted collection series: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection create tx: {error}"))?;
    pool.close().await;

    Ok(collection_id)
}

async fn persist_collection_update(
    database_file: &FsPath,
    collection_id: &str,
    input: &PersistedCollectionWriteInput,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection update db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection update tx: {error}"))?;

    let updated = sqlx::query(
        "UPDATE COLLECTION SET NAME = ?, ORDERED = ?, SERIES_COUNT = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?",
    )
    .bind(&input.name)
    .bind(input.ordered)
    .bind(input.series_ids.len() as i64)
    .bind(collection_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("update persisted collection: {error}"))?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection update tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    replace_collection_series(&mut tx, collection_id, &input.series_ids)
        .await
        .map_err(|error| format!("replace persisted collection series: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit collection update tx: {error}"))?;
    pool.close().await;
    Ok(true)
}

async fn delete_persisted_collection(database_file: &FsPath, collection_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open collection delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin collection delete tx: {error}"))?;

    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete persisted collection series: {error}"))?;

    let deleted = sqlx::query("DELETE FROM COLLECTION WHERE ID = ?")
        .bind(collection_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete persisted collection: {error}"))?
        .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback collection delete tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit collection delete tx: {error}"))?;
    pool.close().await;
    Ok(true)
}

async fn replace_collection_series(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    collection_id: &str,
    series_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM COLLECTION_SERIES WHERE COLLECTION_ID = ?")
        .bind(collection_id)
        .execute(&mut **tx)
        .await?;

    for (index, series_id) in series_ids.iter().enumerate() {
        sqlx::query("INSERT INTO COLLECTION_SERIES (COLLECTION_ID, SERIES_ID, NUMBER) VALUES (?, ?, ?)")
            .bind(collection_id)
            .bind(series_id)
            .bind(index as i64)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

fn generated_collection_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    format!("collection-{nanos}")
}

async fn persist_readlist_create(
    database_file: &FsPath,
    input: &PersistedReadlistWriteInput,
) -> Result<String, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist create db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist create tx: {error}"))?;

    let readlist_id = generated_readlist_id();
    sqlx::query(
        "INSERT INTO READLIST (ID, NAME, BOOK_COUNT, SUMMARY, ORDERED) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&readlist_id)
    .bind(&input.name)
    .bind(input.book_ids.len() as i64)
    .bind(&input.summary)
    .bind(input.ordered)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("insert persisted readlist: {error}"))?;

    replace_readlist_books(&mut tx, &readlist_id, &input.book_ids)
        .await
        .map_err(|error| format!("insert persisted readlist books: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist create tx: {error}"))?;
    pool.close().await;

    Ok(readlist_id)
}

async fn persist_readlist_update(
    database_file: &FsPath,
    readlist_id: &str,
    input: &PersistedReadlistWriteInput,
) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist update db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist update tx: {error}"))?;

    let updated = sqlx::query(
        "UPDATE READLIST SET NAME = ?, SUMMARY = ?, ORDERED = ?, BOOK_COUNT = ?, LAST_MODIFIED_DATE = CURRENT_TIMESTAMP WHERE ID = ?",
    )
    .bind(&input.name)
    .bind(&input.summary)
    .bind(input.ordered)
    .bind(input.book_ids.len() as i64)
    .bind(readlist_id)
    .execute(&mut *tx)
    .await
    .map_err(|error| format!("update persisted readlist: {error}"))?
    .rows_affected()
        > 0;

    if !updated {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist update tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    replace_readlist_books(&mut tx, readlist_id, &input.book_ids)
        .await
        .map_err(|error| format!("replace persisted readlist books: {error}"))?;

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist update tx: {error}"))?;
    pool.close().await;
    Ok(true)
}

async fn delete_persisted_readlist(database_file: &FsPath, readlist_id: &str) -> Result<bool, String> {
    if !database_file.exists() {
        return Ok(false);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open readlist delete db: {error}"))?;
    let mut tx = pool
        .begin()
        .await
        .map_err(|error| format!("begin readlist delete tx: {error}"))?;

    sqlx::query("DELETE FROM THUMBNAIL_READLIST WHERE READLIST_ID = ?")
        .bind(readlist_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete persisted readlist thumbnails: {error}"))?;
    sqlx::query("DELETE FROM READLIST_BOOK WHERE READLIST_ID = ?")
        .bind(readlist_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete persisted readlist books: {error}"))?;

    let deleted = sqlx::query("DELETE FROM READLIST WHERE ID = ?")
        .bind(readlist_id)
        .execute(&mut *tx)
        .await
        .map_err(|error| format!("delete persisted readlist: {error}"))?
        .rows_affected()
        > 0;

    if !deleted {
        tx.rollback()
            .await
            .map_err(|error| format!("rollback readlist delete tx: {error}"))?;
        pool.close().await;
        return Ok(false);
    }

    tx.commit()
        .await
        .map_err(|error| format!("commit readlist delete tx: {error}"))?;
    pool.close().await;
    Ok(true)
}

async fn replace_readlist_books(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    readlist_id: &str,
    book_ids: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query("DELETE FROM READLIST_BOOK WHERE READLIST_ID = ?")
        .bind(readlist_id)
        .execute(&mut **tx)
        .await?;

    for (index, book_id) in book_ids.iter().enumerate() {
        sqlx::query("INSERT INTO READLIST_BOOK (READLIST_ID, BOOK_ID, NUMBER) VALUES (?, ?, ?)")
            .bind(readlist_id)
            .bind(book_id)
            .bind(index as i64)
            .execute(&mut **tx)
            .await?;
    }

    Ok(())
}

fn generated_readlist_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    format!("readlist-{nanos}")
}

fn readlist_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    (!values.is_empty()).then_some(values)
}

fn readlist_author_query_values(query: &str) -> Option<Vec<String>> {
    let raw_values = query_values(query, "author");
    if raw_values.is_empty() {
        return None;
    }

    let authors = raw_values
        .into_iter()
        .filter_map(parse_readlist_author_query_value)
        .collect::<Vec<_>>();

    if authors.is_empty() {
        Some(vec!["__komga_rust_unsupported_author_role__".to_string()])
    } else {
        Some(authors)
    }
}

fn parse_readlist_author_query_value(value: &str) -> Option<String> {
    let mut parts = value.splitn(2, ',');
    let name = parts.next()?.trim();
    if name.is_empty() {
        return None;
    }

    let Some(role) = parts.next() else {
        return Some(name.to_ascii_lowercase());
    };
    let role = role.trim();
    if role.is_empty() || role.eq_ignore_ascii_case("writer") {
        Some(name.to_ascii_lowercase())
    } else {
        None
    }
}

fn parse_optional_query_bool(value: &str) -> Option<bool> {
    if value.eq_ignore_ascii_case("true") {
        Some(true)
    } else if value.eq_ignore_ascii_case("false") {
        Some(false)
    } else {
        None
    }
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

fn classify_readlists_browse_request(
    query_string: &str,
    query: &ReadListsQuery,
) -> Result<(), DiscoveryError> {
    let _ = query_string;
    classify_readlists_browse_query(query)
}

#[derive(Clone, Copy)]
enum ReadListsSort {
    NameAsc,
    NameDesc,
    SearchOrName,
}

fn parse_readlists_sort(value: &str) -> ReadListsSort {
    let mut parts = value.splitn(2, ',');
    let field = parts.next().unwrap_or_default().trim();
    let direction = parts.next().unwrap_or("asc").trim();

    if field.eq_ignore_ascii_case("name") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::NameDesc
        } else {
            ReadListsSort::NameAsc
        }
    } else {
        ReadListsSort::SearchOrName
    }
}

fn readlist_search_score(readlist: &ReadListReadModel, tokens: &[String]) -> usize {
    let name = readlist.name.to_ascii_lowercase();
    let summary = readlist.summary.to_ascii_lowercase();

    tokens
        .iter()
        .map(|token| {
            let name_hits = name.matches(token).count();
            let summary_hits = summary.matches(token).count();
            name_hits + summary_hits
        })
        .sum::<usize>()
}

fn book_detail_payload(book: &BookDetailReadModel, is_admin: bool) -> Value {
    let url = restricted_book_url(&book.url, is_admin);
    let media_profile = media_profile_for_media_type(&book.media_type);

    json!({
        "id": book.id,
        "seriesId": book.series_id,
        "seriesTitle": book.series_title,
        "libraryId": book.library_id,
        "name": book.name,
        "url": url,
        "number": book.number,
        "created": book.created,
        "lastModified": book.last_modified,
        "fileLastModified": book.file_last_modified,
        "sizeBytes": book.size_bytes,
        "size": format_size_bytes(book.size_bytes),
        "media": {
            "status": book.media_status,
            "mediaType": book.media_type,
            "pagesCount": book.media_pages_count,
            "comment": book.media_comment,
            "epubDivinaCompatible": false,
            "epubIsKepub": false,
            "mediaProfile": media_profile
        },
        "metadata": {
            "title": book.metadata_title,
            "titleLock": false,
            "summary": book.metadata_summary,
            "summaryLock": false,
            "number": book.metadata_number,
            "numberLock": false,
            "numberSort": book.metadata_number_sort,
            "numberSortLock": false,
            "releaseDate": book.metadata_release_date,
            "releaseDateLock": false,
            "authors": book.metadata_authors.iter().map(|name| json!({ "name": name, "role": "writer" })).collect::<Vec<_>>(),
            "authorsLock": false,
            "tags": book.metadata_tags,
            "tagsLock": false,
            "isbn": book.metadata_isbn,
            "isbnLock": false,
            "links": [],
            "linksLock": false,
            "created": book.metadata_created,
            "lastModified": book.metadata_last_modified
        },
        "readProgress": book.read_progress.as_ref().map_or(Value::Null, |progress| json!({
            "page": progress.page,
            "completed": progress.completed,
            "readDate": progress.read_date,
            "created": progress.created,
            "lastModified": progress.last_modified,
            "deviceId": progress.device_id,
            "deviceName": progress.device_name,
        })),
        "deleted": book.deleted,
        "fileHash": book.file_hash,
        "oneshot": book.oneshot
    })
}

fn format_size_bytes(size_bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];

    if size_bytes < 1024 {
        return format!("{size_bytes} B");
    }

    let mut size = size_bytes as f64;
    let mut unit_index = 0usize;
    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if (size - size.round()).abs() < 0.05 {
        format!("{} {}", size.round() as u64, UNITS[unit_index])
    } else {
        format!("{size:.1} {}", UNITS[unit_index])
    }
}

fn media_profile_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "application/zip"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => "DIVINA",
        "application/epub+zip" => "EPUB",
        "application/pdf" => "PDF",
        _ => "",
    }
}

fn series_detail_payload(series: &SeriesDetailReadModel, is_admin: bool) -> Value {
    let url = if is_admin {
        series.url.clone()
    } else {
        String::new()
    };

    let mut metadata = Map::new();
    metadata.insert("status".to_string(), Value::String(series.status.clone()));
    metadata.insert("statusLock".to_string(), Value::Bool(false));
    metadata.insert("title".to_string(), Value::String(series.title.clone()));
    metadata.insert("titleLock".to_string(), Value::Bool(false));
    metadata.insert("titleSort".to_string(), Value::String(series.title.clone()));
    metadata.insert("titleSortLock".to_string(), Value::Bool(false));
    metadata.insert("summary".to_string(), Value::String(series.summary.clone()));
    metadata.insert("summaryLock".to_string(), Value::Bool(false));
    metadata.insert(
        "readingDirection".to_string(),
        Value::String(series.reading_direction.clone()),
    );
    metadata.insert("readingDirectionLock".to_string(), Value::Bool(false));
    metadata.insert(
        "publisher".to_string(),
        Value::String(series.publisher.clone()),
    );
    metadata.insert("publisherLock".to_string(), Value::Bool(false));
    metadata.insert(
        "ageRating".to_string(),
        series
            .age_rating
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("ageRatingLock".to_string(), Value::Bool(false));
    metadata.insert(
        "language".to_string(),
        Value::String(series.language.clone()),
    );
    metadata.insert("languageLock".to_string(), Value::Bool(false));
    metadata.insert(
        "genres".to_string(),
        Value::Array(series.genres.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("genresLock".to_string(), Value::Bool(false));
    metadata.insert(
        "tags".to_string(),
        Value::Array(series.tags.iter().cloned().map(Value::String).collect()),
    );
    metadata.insert("tagsLock".to_string(), Value::Bool(false));
    metadata.insert(
        "totalBookCount".to_string(),
        series
            .total_book_count
            .map_or(Value::Null, |it| Value::Number(it.into())),
    );
    metadata.insert("totalBookCountLock".to_string(), Value::Bool(false));
    metadata.insert(
        "sharingLabels".to_string(),
        Value::Array(
            series
                .sharing_labels
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert("sharingLabelsLock".to_string(), Value::Bool(false));
    metadata.insert("links".to_string(), Value::Array(vec![]));
    metadata.insert("linksLock".to_string(), Value::Bool(false));
    metadata.insert(
        "alternateTitles".to_string(),
        Value::Array(
            series
                .alternate_titles
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    metadata.insert("alternateTitlesLock".to_string(), Value::Bool(false));
    metadata.insert(
        "created".to_string(),
        Value::String(series.metadata_created.clone()),
    );
    metadata.insert(
        "lastModified".to_string(),
        Value::String(series.metadata_last_modified.clone()),
    );

    let mut books_metadata = Map::new();
    books_metadata.insert("authors".to_string(), Value::Array(vec![]));
    books_metadata.insert(
        "tags".to_string(),
        Value::Array(
            series
                .books_metadata_tags
                .iter()
                .cloned()
                .map(Value::String)
                .collect(),
        ),
    );
    books_metadata.insert(
        "releaseDate".to_string(),
        series
            .books_metadata_release_date
            .clone()
            .map_or(Value::Null, Value::String),
    );
    books_metadata.insert(
        "summary".to_string(),
        Value::String(series.books_metadata_summary.clone()),
    );
    books_metadata.insert(
        "summaryNumber".to_string(),
        Value::String(series.books_metadata_summary_number.clone()),
    );
    books_metadata.insert(
        "created".to_string(),
        Value::String(series.books_metadata_created.clone()),
    );
    books_metadata.insert(
        "lastModified".to_string(),
        Value::String(series.books_metadata_last_modified.clone()),
    );

    let mut payload = Map::new();
    payload.insert("id".to_string(), Value::String(series.id.clone()));
    payload.insert(
        "libraryId".to_string(),
        Value::String(series.library_id.clone()),
    );
    payload.insert("name".to_string(), Value::String(series.title.clone()));
    payload.insert("url".to_string(), Value::String(url));
    payload.insert("created".to_string(), Value::String(series.created.clone()));
    payload.insert(
        "lastModified".to_string(),
        Value::String(series.last_modified.clone()),
    );
    payload.insert(
        "fileLastModified".to_string(),
        Value::String(series.file_last_modified.clone()),
    );
    payload.insert(
        "booksCount".to_string(),
        Value::Number(series.books_count.into()),
    );
    payload.insert(
        "booksReadCount".to_string(),
        Value::Number(series.books_read_count.into()),
    );
    payload.insert(
        "booksUnreadCount".to_string(),
        Value::Number(series.books_unread_count.into()),
    );
    payload.insert(
        "booksInProgressCount".to_string(),
        Value::Number(series.books_in_progress_count.into()),
    );
    payload.insert("metadata".to_string(), Value::Object(metadata));
    payload.insert("booksMetadata".to_string(), Value::Object(books_metadata));
    payload.insert("deleted".to_string(), Value::Bool(series.deleted));
    payload.insert("oneshot".to_string(), Value::Bool(series.oneshot));

    Value::Object(payload)
}

fn series_collections_payload(collections: &[CollectionReadModel]) -> Value {
    Value::Array(collections.iter().map(collection_payload).collect())
}

fn readlists_payload(readlists: &[ReadListReadModel]) -> Value {
    Value::Array(readlists.iter().map(readlist_payload).collect())
}

fn readlists_page_payload(page: PageEnvelope<ReadListReadModel>) -> Value {
    let content = page
        .content
        .iter()
        .map(readlist_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = page.page.saturating_mul(page.size);

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": true,
            "unpaged": false
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn collections_page_payload(page: PageEnvelope<CollectionReadModel>) -> Value {
    let content = page
        .content
        .iter()
        .map(collection_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len();
    let first = page.page == 0;
    let last = page.total_pages == 0 || page.page + 1 >= page.total_pages;
    let offset = page.page.saturating_mul(page.size);

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page.page,
            "pageSize": page.size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": offset,
            "paged": true,
            "unpaged": false
        },
        "last": last,
        "totalElements": page.total_elements,
        "totalPages": page.total_pages,
        "first": first,
        "size": page.size,
        "number": page.page,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

fn collection_payload(collection: &CollectionReadModel) -> Value {
    json!({
        "id": collection.id,
        "name": collection.name,
        "ordered": collection.ordered,
        "seriesIds": collection.series_ids,
        "createdDate": collection.created_date,
        "lastModifiedDate": collection.last_modified_date,
        "filtered": collection.filtered,
    })
}

fn collection_series_payload(series: &[PersistedCollectionSeriesReadModel]) -> Value {
    Value::Array(
        series
            .iter()
            .map(|series| {
                json!({
                    "id": series.id,
                    "libraryId": series.library_id,
                    "name": series.name,
                    "metadata": {
                        "title": series.title,
                        "sharingLabels": []
                    },
                    "deleted": series.deleted,
                    "oneshot": series.oneshot,
                })
            })
            .collect(),
    )
}

fn readlist_payload(readlist: &ReadListReadModel) -> Value {
    json!({
        "id": readlist.id,
        "name": readlist.name,
        "summary": readlist.summary,
        "ordered": readlist.ordered,
        "bookIds": readlist.book_ids,
        "createdDate": readlist.created_date,
        "lastModifiedDate": readlist.last_modified_date,
        "filtered": readlist.filtered,
    })
}

#[cfg(test)]
mod tests {
    use super::decode_query_component;

    #[test]
    fn decode_query_component_decodes_percent_encoded_utf8_sequences() {
        assert_eq!(decode_query_component("caf%C3%A9+au+lait"), "café au lait");
    }
}
