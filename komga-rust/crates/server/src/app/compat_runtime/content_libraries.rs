use std::io::Read;
use std::path::Path as FsPath;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_domain::discovery::DiscoveryError;
use komga_persistence::read_models::libraries::{
    PersistedLibraryReadModel, get_persisted_library, list_persisted_libraries,
};
use komga_persistence::sqlite::connect_pool;
use serde_json::{Value, json};
use sqlx::{Row, Sqlite, Transaction};

use crate::app::CompatProfile;
use crate::app::discovery_auth::{DiscoveryAuthState, DiscoveryQueryContext};
use crate::app::runtime_auth::{require_admin, require_auth};
use crate::task_queue::TaskQueueRecord;

use super::super::OperationalState;
use super::helpers::to_domain_query_context;
use super::mark_native;

pub(super) async fn response(
    _profile: CompatProfile,
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, None) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    native_owned_libraries_response(context, database_file).await
}

pub(super) async fn library_detail(
    _profile: CompatProfile,
    headers: HeaderMap,
    auth_state: DiscoveryAuthState,
    database_file: &FsPath,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let context = match auth_state.resolve_query_context(&headers, Some(&[library_id.clone()])) {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };

    native_owned_library_detail_response(context, database_file, &library_id).await
}

pub(super) async fn library_update(
    headers: HeaderMap,
    database_file: &FsPath,
    state: OperationalState,
    Path(library_id): Path<String>,
    body: Value,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let mut library = match load_persisted_library(database_file, &library_id).await {
        Ok(Some(library)) => library,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };
    let previous_library = library.clone();

    let body = match body.as_object() {
        Some(body) => body,
        None => return bad_request_response("library update payload must be a JSON object"),
    };

    if let Err(response) = apply_library_body(&mut library, body) {
        return response;
    }
    if let Err(message) = validate_library_before_persist(database_file, &library).await {
        return bad_request_response(message.as_str());
    }

    match persist_library_update(database_file, &library).await {
        Ok(true) => {
            let mut task_records = Vec::new();
            if library_should_rescan(&previous_library, &library) {
                task_records.push(scan_library_task_record(&library.id, false));
            }
            if library.hash_files && !previous_library.hash_files {
                match library_book_ids_with_empty_hash(database_file, &library.id, false).await {
                    Ok(book_ids) => task_records.extend(book_ids.into_iter().map(|book_id| {
                        TaskQueueRecord::new(format!("HASH_BOOK:{book_id}"), 10, Some(book_id))
                    })),
                    Err(error) => return internal_error_response(error),
                }
            }
            if library.hash_koreader && !previous_library.hash_koreader {
                match library_book_ids_with_empty_hash(database_file, &library.id, true).await {
                    Ok(book_ids) => task_records.extend(book_ids.into_iter().map(|book_id| {
                        TaskQueueRecord::new(
                            format!("HASH_BOOK_KOREADER:{book_id}"),
                            10,
                            Some(book_id),
                        )
                    })),
                    Err(error) => return internal_error_response(error),
                }
            }
            if library.hash_pages && !previous_library.hash_pages {
                task_records.push(TaskQueueRecord::new(
                    format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH:{}", library.id),
                    10,
                    Some(library.id.clone()),
                ));
            }
            if library.repair_extensions && !previous_library.repair_extensions {
                task_records.push(TaskQueueRecord::new(
                    format!("REPAIR_EXTENSIONS:{}", library.id),
                    10,
                    Some(library.id.clone()),
                ));
            }
            if library.convert_to_cbz && !previous_library.convert_to_cbz {
                task_records.push(TaskQueueRecord::new(
                    format!("FIND_BOOKS_TO_CONVERT:{}", library.id),
                    10,
                    Some(library.id.clone()),
                ));
            }

            if task_records.is_empty() {
                StatusCode::NO_CONTENT.into_response()
            } else {
                enqueue_task_records_with_status(&state, task_records, StatusCode::NO_CONTENT)
            }
        }
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

async fn library_book_ids_with_empty_hash(
    database_file: &FsPath,
    library_id: &str,
    koreader: bool,
) -> Result<Vec<String>, String> {
    if !database_file.exists() {
        return Ok(vec![]);
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open library hash query db: {error}"))?;

    let sql = if koreader {
        "SELECT ID \
         FROM BOOK \
         WHERE LIBRARY_ID = ? \
         AND DELETED_DATE IS NULL \
         AND (FILE_HASH_KOREADER = '' OR FILE_HASH_KOREADER IS NULL)"
    } else {
        "SELECT ID \
         FROM BOOK \
         WHERE LIBRARY_ID = ? \
         AND DELETED_DATE IS NULL \
         AND (FILE_HASH = '' OR FILE_HASH IS NULL)"
    };

    let rows = sqlx::query(sql)
        .bind(library_id)
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query books with empty hash: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect::<Vec<_>>())
}

pub(super) async fn library_create(
    headers: HeaderMap,
    database_file: &FsPath,
    state: OperationalState,
    body: Value,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let body = match body.as_object() {
        Some(body) => body,
        None => return bad_request_response("library create payload must be a JSON object"),
    };

    if !body.contains_key("name") || !body.contains_key("root") {
        return bad_request_response("library create payload must include name and root");
    }

    let mut library = PersistedLibrary::fallback_default();
    library.id = generated_library_id();

    if let Err(response) = apply_library_body(&mut library, body) {
        return response;
    }
    if library.name.trim().is_empty() || library.root.trim().is_empty() {
        return bad_request_response("library create payload must provide non-empty name and root");
    }
    if let Err(message) = validate_library_before_persist(database_file, &library).await {
        return bad_request_response(message.as_str());
    }

    match persist_library_create(database_file, &library).await {
        Ok(()) => {
            let enqueue_response =
                enqueue_task_records(&state, vec![scan_library_task_record(&library.id, false)]);
            if enqueue_response.status().is_server_error() {
                return enqueue_response;
            }
            Json(library_payload(library, true)).into_response()
        }
        Err(error) => internal_error_response(error),
    }
}

pub(super) async fn library_delete(
    headers: HeaderMap,
    database_file: &FsPath,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match delete_persisted_library(database_file, &library_id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

pub(super) async fn library_scan(
    headers: HeaderMap,
    uri: Uri,
    database_file: &FsPath,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let deep_scan = uri.query().map(is_deep_scan_query).unwrap_or(false);

    match load_persisted_library(database_file, &library_id).await {
        Ok(Some(_)) => enqueue_task_records(
            &state,
            vec![scan_library_task_record(&library_id, deep_scan)],
        ),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

fn is_deep_scan_query(query: &str) -> bool {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .any(|(key, value)| {
            key.eq_ignore_ascii_case("deep")
                && matches!(value.to_ascii_lowercase().as_str(), "true" | "1")
        })
}

fn library_should_rescan(previous: &PersistedLibrary, next: &PersistedLibrary) -> bool {
    previous.root != next.root
        || previous.scan_force_modified_time != next.scan_force_modified_time
        || previous.scan_cbx != next.scan_cbx
        || previous.scan_pdf != next.scan_pdf
        || previous.scan_epub != next.scan_epub
        || previous.oneshots_directory != next.oneshots_directory
        || previous.scan_directory_exclusions != next.scan_directory_exclusions
}

pub(super) async fn library_analyze(
    headers: HeaderMap,
    database_file: &FsPath,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let task_records = match library_book_ids(database_file, &library_id).await {
        Ok(Some(book_ids)) => {
            let mut task_records = Vec::with_capacity(book_ids.len());
            task_records.extend(book_ids.into_iter().map(|book_id| {
                TaskQueueRecord::new(format!("ANALYZE_BOOK:{book_id}"), 90, Some(book_id))
            }));
            task_records
        }
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    enqueue_task_records(&state, task_records)
}

pub(super) async fn library_metadata_refresh(
    headers: HeaderMap,
    database_file: &FsPath,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let (series_ids, book_ids) = match library_series_and_book_ids(database_file, &library_id).await
    {
        Ok(Some(ids)) => ids,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    let mut task_records = Vec::with_capacity((book_ids.len() * 2) + series_ids.len());
    for book_id in book_ids {
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_BOOK_METADATA:{book_id}"),
            80,
            Some(book_id.clone()),
        ));
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_BOOK_LOCAL_ARTWORK:{book_id}"),
            80,
            Some(book_id),
        ));
    }
    for series_id in series_ids {
        task_records.push(TaskQueueRecord::new(
            format!("REFRESH_SERIES_LOCAL_ARTWORK:{series_id}"),
            80,
            Some(series_id),
        ));
    }

    enqueue_task_records(&state, task_records)
}

pub(super) async fn library_empty_trash(
    headers: HeaderMap,
    database_file: &FsPath,
    state: OperationalState,
    Path(library_id): Path<String>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    match load_persisted_library(database_file, &library_id).await {
        Ok(Some(_)) => enqueue_task_records(
            &state,
            vec![
                TaskQueueRecord::new(
                    format!("EMPTY_TRASH:{library_id}"),
                    70,
                    Some(library_id.clone()),
                ),
                scan_library_task_record(&library_id, false),
            ],
        ),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => internal_error_response(error),
    }
}

fn scan_library_task_record(library_id: &str, deep_scan: bool) -> TaskQueueRecord {
    let mut task = TaskQueueRecord::new(
        format!("SCAN_LIBRARY:{library_id}"),
        100,
        Some(library_id.to_string()),
    );
    if deep_scan {
        task = task.with_payload(r#"{"deep":true}"#);
    }
    task
}

fn enqueue_task_records(state: &OperationalState, task_records: Vec<TaskQueueRecord>) -> Response {
    enqueue_task_records_with_status(state, task_records, StatusCode::ACCEPTED)
}

fn enqueue_task_records_with_status(
    state: &OperationalState,
    task_records: Vec<TaskQueueRecord>,
    status: StatusCode,
) -> Response {
    let mut task_queue = state
        .task_queue
        .lock()
        .expect("task queue state lock should not be poisoned");
    for task in task_records {
        task_queue.enqueue(task);
    }

    if let Err(error) = task_queue.process_available(&state.runtime) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error.to_string() })),
        )
            .into_response();
    }

    let mut response = status.into_response();
    mark_native(&mut response);
    response
}

async fn library_book_ids(
    database_file: &FsPath,
    library_id: &str,
) -> Result<Option<Vec<String>>, sqlx::Error> {
    let Some(_) = load_persisted_library(database_file, library_id).await? else {
        return Ok(None);
    };

    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        "SELECT ID \
                            FROM BOOK \
                            WHERE LIBRARY_ID = ? \
                            ORDER BY ID ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;

    Ok(Some(
        rows.into_iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect(),
    ))
}

async fn library_series_and_book_ids(
    database_file: &FsPath,
    library_id: &str,
) -> Result<Option<(Vec<String>, Vec<String>)>, sqlx::Error> {
    let Some(_) = load_persisted_library(database_file, library_id).await? else {
        return Ok(None);
    };

    let pool = connect_pool(database_file, 1).await?;
    let series_rows = sqlx::query(
        "SELECT ID \
                                   FROM SERIES \
                                   WHERE LIBRARY_ID = ? \
                                   ORDER BY ID ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;
    let book_rows = sqlx::query(
        "SELECT ID \
                                 FROM BOOK \
                                 WHERE LIBRARY_ID = ? \
                                 ORDER BY ID ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;

    Ok(Some((
        series_rows
            .into_iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect(),
        book_rows
            .into_iter()
            .map(|row| row.get::<String, _>("ID"))
            .collect(),
    )))
}

fn generated_library_id() -> String {
    format!("library-{}", random_hex_token(12))
}

fn random_hex_token(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let _ = file.read_exact(&mut bytes);
    } else {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        for (index, byte) in bytes.iter_mut().enumerate() {
            *byte = ((seed >> ((index % 8) * 8)) as u8) ^ (index as u8).wrapping_mul(17);
        }
    }

    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn apply_library_body(
    library: &mut PersistedLibrary,
    body: &serde_json::Map<String, Value>,
) -> Result<(), Response> {
    apply_string_field(body, "name", &mut library.name)?;
    apply_string_field(body, "root", &mut library.root)?;
    apply_bool_field(
        body,
        "importComicInfoBook",
        &mut library.import_comicinfo_book,
    )?;
    apply_bool_field(
        body,
        "importComicInfoSeries",
        &mut library.import_comicinfo_series,
    )?;
    apply_bool_field(
        body,
        "importComicInfoCollection",
        &mut library.import_comicinfo_collection,
    )?;
    apply_bool_field(
        body,
        "importComicInfoReadList",
        &mut library.import_comicinfo_readlist,
    )?;
    apply_bool_field(
        body,
        "importComicInfoSeriesAppendVolume",
        &mut library.import_comicinfo_series_append_volume,
    )?;
    apply_bool_field(body, "importEpubBook", &mut library.import_epub_book)?;
    apply_bool_field(body, "importEpubSeries", &mut library.import_epub_series)?;
    apply_bool_field(body, "importMylarSeries", &mut library.import_mylar_series)?;
    apply_bool_field(
        body,
        "importLocalArtwork",
        &mut library.import_local_artwork,
    )?;
    apply_bool_field(body, "importBarcodeIsbn", &mut library.import_barcode_isbn)?;
    apply_bool_field(
        body,
        "scanForceModifiedTime",
        &mut library.scan_force_modified_time,
    )?;
    apply_string_field(body, "scanInterval", &mut library.scan_interval)?;
    apply_bool_field(body, "scanOnStartup", &mut library.scan_on_startup)?;
    apply_bool_field(body, "scanCbx", &mut library.scan_cbx)?;
    apply_bool_field(body, "scanPdf", &mut library.scan_pdf)?;
    apply_bool_field(body, "scanEpub", &mut library.scan_epub)?;
    apply_string_array_field(
        body,
        "scanDirectoryExclusions",
        &mut library.scan_directory_exclusions,
    )?;
    apply_bool_field(body, "repairExtensions", &mut library.repair_extensions)?;
    apply_bool_field(body, "convertToCbz", &mut library.convert_to_cbz)?;
    apply_bool_field(
        body,
        "emptyTrashAfterScan",
        &mut library.empty_trash_after_scan,
    )?;
    apply_string_field(body, "seriesCover", &mut library.series_cover)?;
    apply_bool_field(body, "hashFiles", &mut library.hash_files)?;
    apply_bool_field(body, "hashPages", &mut library.hash_pages)?;
    apply_bool_field(body, "hashKoreader", &mut library.hash_koreader)?;
    apply_bool_field(body, "analyzeDimensions", &mut library.analyze_dimensions)?;
    apply_optional_string_field(body, "oneshotsDirectory", &mut library.oneshots_directory)?;
    Ok(())
}

fn apply_string_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut String,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!("{key} must be a string")));
    };
    *field = value.to_string();
    Ok(())
}

fn apply_bool_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut bool,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_bool() else {
        return Err(bad_request_response(&format!("{key} must be a boolean")));
    };
    *field = value;
    Ok(())
}

fn apply_optional_string_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<String>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    if value.is_null() {
        *field = None;
        return Ok(());
    }
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!(
            "{key} must be a string or null"
        )));
    };
    *field = if value.chars().all(|ch| ch.is_whitespace()) {
        None
    } else {
        Some(value.to_string())
    };
    Ok(())
}

fn apply_string_array_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Vec<String>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(values) = value.as_array() else {
        return Err(bad_request_response(&format!(
            "{key} must be an array of strings"
        )));
    };
    let mut next = Vec::with_capacity(values.len());
    for entry in values {
        let Some(entry) = entry.as_str() else {
            return Err(bad_request_response(&format!(
                "{key} must be an array of strings"
            )));
        };
        next.push(entry.to_string());
    }
    *field = next;
    Ok(())
}

async fn persist_library_create(
    database_file: &FsPath,
    library: &PersistedLibrary,
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    insert_library_row(&mut tx, library).await?;
    replace_library_exclusions(&mut tx, &library.id, &library.scan_directory_exclusions).await?;
    tx.commit().await?;
    Ok(())
}

async fn persist_library_update(
    database_file: &FsPath,
    library: &PersistedLibrary,
) -> Result<bool, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    let updated = update_library_row(&mut tx, library).await?;
    if !updated {
        tx.rollback().await?;
        return Ok(false);
    }
    replace_library_exclusions(&mut tx, &library.id, &library.scan_directory_exclusions).await?;
    tx.commit().await?;
    Ok(true)
}

async fn delete_persisted_library(
    database_file: &FsPath,
    library_id: &str,
) -> Result<bool, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    let exists = sqlx::query(
        "SELECT 1 \
         FROM LIBRARY \
         WHERE ID = ? \
         LIMIT 1",
    )
    .bind(library_id)
    .fetch_optional(&mut *tx)
    .await?
    .is_some();
    if !exists {
        tx.rollback().await?;
        return Ok(false);
    }

    for sql in [
        "DELETE FROM LIBRARY_EXCLUSIONS WHERE LIBRARY_ID = ?",
        "DELETE FROM SIDECAR WHERE LIBRARY_ID = ?",
        "DELETE FROM COLLECTION_SERIES WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM READ_PROGRESS_SERIES WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM THUMBNAIL_SERIES WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM SERIES_METADATA_ALTERNATE_TITLE WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM SERIES_METADATA_GENRE WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM SERIES_METADATA_LINK WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM SERIES_METADATA_SHARING WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM SERIES_METADATA_TAG WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM SERIES_METADATA WHERE SERIES_ID IN (SELECT ID FROM SERIES WHERE LIBRARY_ID = ?)",
        "DELETE FROM READLIST_BOOK WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM READ_PROGRESS WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM THUMBNAIL_BOOK WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM MEDIA_PAGE WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM MEDIA_FILE WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM MEDIA WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM BOOK_METADATA_AUTHOR WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM BOOK_METADATA_LINK WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM BOOK_METADATA_TAG WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM BOOK_METADATA WHERE BOOK_ID IN (SELECT ID FROM BOOK WHERE LIBRARY_ID = ?)",
        "DELETE FROM BOOK WHERE LIBRARY_ID = ?",
        "DELETE FROM SERIES WHERE LIBRARY_ID = ?",
    ] {
        sqlx::query(sql).bind(library_id).execute(&mut *tx).await?;
    }

    let deleted = sqlx::query(
        "DELETE \
                                FROM LIBRARY \
                                WHERE ID = ?",
    )
    .bind(library_id)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        > 0;
    if !deleted {
        tx.rollback().await?;
        return Ok(false);
    }
    tx.commit().await?;
    Ok(true)
}

async fn load_persisted_library(
    database_file: &FsPath,
    library_id: &str,
) -> Result<Option<PersistedLibrary>, sqlx::Error> {
    if !database_file.exists() {
        return Ok(None);
    }

    let pool = connect_pool(database_file, 1).await?;
    let row = sqlx::query(
        "SELECT ID, NAME, ROOT, IMPORT_COMICINFO_BOOK, IMPORT_COMICINFO_SERIES, \
                IMPORT_COMICINFO_COLLECTION, IMPORT_COMICINFO_READLIST, \
                IMPORT_COMICINFO_SERIES_APPEND_VOLUME, IMPORT_EPUB_BOOK, IMPORT_EPUB_SERIES, \
                IMPORT_MYLAR_SERIES, IMPORT_LOCAL_ARTWORK, IMPORT_BARCODE_ISBN, \
                SCAN_FORCE_MODIFIED_TIME, SCAN_INTERVAL, SCAN_STARTUP, SCAN_CBX, SCAN_PDF, \
                SCAN_EPUB, REPAIR_EXTENSIONS, CONVERT_TO_CBZ, EMPTY_TRASH_AFTER_SCAN, \
                SERIES_COVER, HASH_FILES, HASH_PAGES, HASH_KOREADER, ANALYZE_DIMENSIONS, \
                ONESHOTS_DIRECTORY, UNAVAILABLE_DATE \
         FROM LIBRARY \
         WHERE ID = ?",
    )
    .bind(library_id)
    .fetch_optional(&pool)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let mut library = map_persisted_library_row(row);
    let exclusions = sqlx::query(
        "SELECT EXCLUSION \
         FROM LIBRARY_EXCLUSIONS \
         WHERE LIBRARY_ID = ? \
         ORDER BY EXCLUSION COLLATE NOCASE ASC",
    )
    .bind(library_id)
    .fetch_all(&pool)
    .await?;
    library.scan_directory_exclusions = exclusions
        .into_iter()
        .map(|row| row.get::<String, _>("EXCLUSION"))
        .collect();

    Ok(Some(library))
}

async fn insert_library_row(
    tx: &mut Transaction<'_, Sqlite>,
    library: &PersistedLibrary,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO LIBRARY ( ID, NAME, ROOT, IMPORT_COMICINFO_BOOK, IMPORT_COMICINFO_SERIES, \
           IMPORT_COMICINFO_COLLECTION, IMPORT_COMICINFO_READLIST, \
           IMPORT_COMICINFO_SERIES_APPEND_VOLUME, IMPORT_EPUB_BOOK, IMPORT_EPUB_SERIES, \
           IMPORT_MYLAR_SERIES, IMPORT_LOCAL_ARTWORK, IMPORT_BARCODE_ISBN, \
           SCAN_FORCE_MODIFIED_TIME, SCAN_INTERVAL, SCAN_STARTUP, SCAN_CBX, SCAN_PDF, SCAN_EPUB, \
           REPAIR_EXTENSIONS, CONVERT_TO_CBZ, EMPTY_TRASH_AFTER_SCAN, SERIES_COVER, HASH_FILES, \
           HASH_PAGES, HASH_KOREADER, ANALYZE_DIMENSIONS, ONESHOTS_DIRECTORY ) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, \
                ?)",
    )
    .bind(&library.id)
    .bind(&library.name)
    .bind(&library.root)
    .bind(library.import_comicinfo_book)
    .bind(library.import_comicinfo_series)
    .bind(library.import_comicinfo_collection)
    .bind(library.import_comicinfo_readlist)
    .bind(library.import_comicinfo_series_append_volume)
    .bind(library.import_epub_book)
    .bind(library.import_epub_series)
    .bind(library.import_mylar_series)
    .bind(library.import_local_artwork)
    .bind(library.import_barcode_isbn)
    .bind(library.scan_force_modified_time)
    .bind(&library.scan_interval)
    .bind(library.scan_on_startup)
    .bind(library.scan_cbx)
    .bind(library.scan_pdf)
    .bind(library.scan_epub)
    .bind(library.repair_extensions)
    .bind(library.convert_to_cbz)
    .bind(library.empty_trash_after_scan)
    .bind(&library.series_cover)
    .bind(library.hash_files)
    .bind(library.hash_pages)
    .bind(library.hash_koreader)
    .bind(library.analyze_dimensions)
    .bind(&library.oneshots_directory)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_library_row(
    tx: &mut Transaction<'_, Sqlite>,
    library: &PersistedLibrary,
) -> Result<bool, sqlx::Error> {
    let updated = sqlx::query(
        "UPDATE LIBRARY \
         SET LAST_MODIFIED_DATE = CURRENT_TIMESTAMP, NAME = ?, ROOT = ?, \
             IMPORT_COMICINFO_BOOK = ?, IMPORT_COMICINFO_SERIES = ?, \
             IMPORT_COMICINFO_COLLECTION = ?, IMPORT_COMICINFO_READLIST = ?, \
             IMPORT_COMICINFO_SERIES_APPEND_VOLUME = ?, IMPORT_EPUB_BOOK = ?, \
             IMPORT_EPUB_SERIES = ?, IMPORT_MYLAR_SERIES = ?, IMPORT_LOCAL_ARTWORK = ?, \
             IMPORT_BARCODE_ISBN = ?, SCAN_FORCE_MODIFIED_TIME = ?, SCAN_INTERVAL = ?, \
             SCAN_STARTUP = ?, SCAN_CBX = ?, SCAN_PDF = ?, SCAN_EPUB = ?, REPAIR_EXTENSIONS = ?, \
             CONVERT_TO_CBZ = ?, EMPTY_TRASH_AFTER_SCAN = ?, SERIES_COVER = ?, HASH_FILES = ?, \
             HASH_PAGES = ?, HASH_KOREADER = ?, ANALYZE_DIMENSIONS = ?, ONESHOTS_DIRECTORY = ? \
         WHERE ID = ?",
    )
    .bind(&library.name)
    .bind(&library.root)
    .bind(library.import_comicinfo_book)
    .bind(library.import_comicinfo_series)
    .bind(library.import_comicinfo_collection)
    .bind(library.import_comicinfo_readlist)
    .bind(library.import_comicinfo_series_append_volume)
    .bind(library.import_epub_book)
    .bind(library.import_epub_series)
    .bind(library.import_mylar_series)
    .bind(library.import_local_artwork)
    .bind(library.import_barcode_isbn)
    .bind(library.scan_force_modified_time)
    .bind(&library.scan_interval)
    .bind(library.scan_on_startup)
    .bind(library.scan_cbx)
    .bind(library.scan_pdf)
    .bind(library.scan_epub)
    .bind(library.repair_extensions)
    .bind(library.convert_to_cbz)
    .bind(library.empty_trash_after_scan)
    .bind(&library.series_cover)
    .bind(library.hash_files)
    .bind(library.hash_pages)
    .bind(library.hash_koreader)
    .bind(library.analyze_dimensions)
    .bind(&library.oneshots_directory)
    .bind(&library.id)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        > 0;
    Ok(updated)
}

async fn replace_library_exclusions(
    tx: &mut Transaction<'_, Sqlite>,
    library_id: &str,
    exclusions: &[String],
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE \
                 FROM LIBRARY_EXCLUSIONS \
                 WHERE LIBRARY_ID = ?",
    )
    .bind(library_id)
    .execute(&mut **tx)
    .await?;

    for exclusion in exclusions {
        sqlx::query(
            "INSERT INTO LIBRARY_EXCLUSIONS (LIBRARY_ID, EXCLUSION) \
                     VALUES (?, ?)",
        )
        .bind(library_id)
        .bind(exclusion)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

fn bad_request_response(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
}

async fn validate_library_before_persist(
    database_file: &FsPath,
    library: &PersistedLibrary,
) -> Result<(), String> {
    let root_path = std::path::Path::new(&library.root);
    if !root_path.exists() {
        return Err("library root does not exist".to_string());
    }
    if !root_path.is_dir() {
        return Err("library root must be a directory".to_string());
    }

    if !database_file.exists() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open library validation db: {error}"))?;
    let rows = sqlx::query(
        "SELECT ID, NAME, ROOT \
         FROM LIBRARY",
    )
    .fetch_all(&pool)
    .await
    .map_err(|error| format!("query library validation rows: {error}"))?;

    let normalized_root = normalize_library_root(&library.root);
    for row in rows {
        let existing_id = row.get::<String, _>("ID");
        if existing_id == library.id {
            continue;
        }

        let existing_name = row.get::<String, _>("NAME");
        if existing_name == library.name {
            return Err("library name must be unique".to_string());
        }

        let normalized_existing = normalize_library_root(&row.get::<String, _>("ROOT"));
        if normalized_root == normalized_existing
            || normalized_root.starts_with(&(normalized_existing.clone() + "/"))
            || normalized_existing.starts_with(&(normalized_root.clone() + "/"))
        {
            return Err("library root cannot overlap another library root".to_string());
        }
    }

    Ok(())
}

fn normalize_library_root(root: &str) -> String {
    root.trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn internal_error_response(error: impl std::fmt::Display) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": error.to_string() })),
    )
        .into_response()
}

fn discovery_error_message(error: &DiscoveryError) -> String {
    match error {
        DiscoveryError::NonNativeRequestShape(details) => format!("{details:?}"),
        DiscoveryError::InvalidRequest(message) | DiscoveryError::Persistence(message) => {
            message.clone()
        }
    }
}

async fn native_owned_libraries_response(
    context: DiscoveryQueryContext,
    database_file: &FsPath,
) -> Response {
    match native_owned_libraries(context.clone(), database_file).await {
        Ok(libraries) => {
            let mut response = Json(libraries_payload(libraries, context.is_admin)).into_response();
            mark_native(&mut response);
            response
        }
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
    }
}

async fn native_owned_library_detail_response(
    context: DiscoveryQueryContext,
    database_file: &FsPath,
    library_id: &str,
) -> Response {
    let domain_context = to_domain_query_context(context.clone());
    match get_persisted_library(database_file, &domain_context, library_id).await {
        Ok(Some(library)) => {
            let library = PersistedLibrary::from(library);
            let mut response = Json(library_payload(library, context.is_admin)).into_response();
            mark_native(&mut response);
            response
        }
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": discovery_error_message(&error) })),
        )
            .into_response(),
    }
}

#[derive(Clone, Debug)]
struct PersistedLibrary {
    id: String,
    name: String,
    root: String,
    import_comicinfo_book: bool,
    import_comicinfo_series: bool,
    import_comicinfo_collection: bool,
    import_comicinfo_readlist: bool,
    import_comicinfo_series_append_volume: bool,
    import_epub_book: bool,
    import_epub_series: bool,
    import_mylar_series: bool,
    import_local_artwork: bool,
    import_barcode_isbn: bool,
    scan_force_modified_time: bool,
    scan_interval: String,
    scan_on_startup: bool,
    scan_cbx: bool,
    scan_pdf: bool,
    scan_epub: bool,
    scan_directory_exclusions: Vec<String>,
    repair_extensions: bool,
    convert_to_cbz: bool,
    empty_trash_after_scan: bool,
    series_cover: String,
    hash_files: bool,
    hash_pages: bool,
    hash_koreader: bool,
    analyze_dimensions: bool,
    oneshots_directory: Option<String>,
    unavailable: bool,
}

impl PersistedLibrary {
    fn fallback_default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            root: String::new(),
            import_comicinfo_book: true,
            import_comicinfo_series: true,
            import_comicinfo_collection: true,
            import_comicinfo_readlist: true,
            import_comicinfo_series_append_volume: true,
            import_epub_book: true,
            import_epub_series: true,
            import_mylar_series: true,
            import_local_artwork: true,
            import_barcode_isbn: true,
            scan_force_modified_time: false,
            scan_interval: "EVERY_6H".to_string(),
            scan_on_startup: false,
            scan_cbx: true,
            scan_pdf: true,
            scan_epub: true,
            scan_directory_exclusions: vec![],
            repair_extensions: false,
            convert_to_cbz: false,
            empty_trash_after_scan: false,
            series_cover: "FIRST".to_string(),
            hash_files: true,
            hash_pages: false,
            hash_koreader: false,
            analyze_dimensions: true,
            oneshots_directory: None,
            unavailable: false,
        }
    }
}

impl From<PersistedLibraryReadModel> for PersistedLibrary {
    fn from(value: PersistedLibraryReadModel) -> Self {
        Self {
            id: value.id,
            name: value.name,
            root: value.root,
            import_comicinfo_book: value.import_comicinfo_book,
            import_comicinfo_series: value.import_comicinfo_series,
            import_comicinfo_collection: value.import_comicinfo_collection,
            import_comicinfo_readlist: value.import_comicinfo_readlist,
            import_comicinfo_series_append_volume: value.import_comicinfo_series_append_volume,
            import_epub_book: value.import_epub_book,
            import_epub_series: value.import_epub_series,
            import_mylar_series: value.import_mylar_series,
            import_local_artwork: value.import_local_artwork,
            import_barcode_isbn: value.import_barcode_isbn,
            scan_force_modified_time: value.scan_force_modified_time,
            scan_interval: value.scan_interval,
            scan_on_startup: value.scan_on_startup,
            scan_cbx: value.scan_cbx,
            scan_pdf: value.scan_pdf,
            scan_epub: value.scan_epub,
            scan_directory_exclusions: value.scan_directory_exclusions,
            repair_extensions: value.repair_extensions,
            convert_to_cbz: value.convert_to_cbz,
            empty_trash_after_scan: value.empty_trash_after_scan,
            series_cover: value.series_cover,
            hash_files: value.hash_files,
            hash_pages: value.hash_pages,
            hash_koreader: value.hash_koreader,
            analyze_dimensions: value.analyze_dimensions,
            oneshots_directory: value.oneshots_directory,
            unavailable: value.unavailable,
        }
    }
}

async fn native_owned_libraries(
    context: DiscoveryQueryContext,
    database_file: &FsPath,
) -> Result<Vec<PersistedLibrary>, DiscoveryError> {
    let domain_context = to_domain_query_context(context);
    let libraries = list_persisted_libraries(database_file, &domain_context).await?;
    Ok(libraries.into_iter().map(PersistedLibrary::from).collect())
}

fn map_persisted_library_row(row: sqlx::sqlite::SqliteRow) -> PersistedLibrary {
    PersistedLibrary {
        id: row.get::<String, _>("ID"),
        name: row.get::<String, _>("NAME"),
        root: row.get::<String, _>("ROOT"),
        import_comicinfo_book: row.get::<bool, _>("IMPORT_COMICINFO_BOOK"),
        import_comicinfo_series: row.get::<bool, _>("IMPORT_COMICINFO_SERIES"),
        import_comicinfo_collection: row.get::<bool, _>("IMPORT_COMICINFO_COLLECTION"),
        import_comicinfo_readlist: row.get::<bool, _>("IMPORT_COMICINFO_READLIST"),
        import_comicinfo_series_append_volume: row
            .get::<bool, _>("IMPORT_COMICINFO_SERIES_APPEND_VOLUME"),
        import_epub_book: row.get::<bool, _>("IMPORT_EPUB_BOOK"),
        import_epub_series: row.get::<bool, _>("IMPORT_EPUB_SERIES"),
        import_mylar_series: row.get::<bool, _>("IMPORT_MYLAR_SERIES"),
        import_local_artwork: row.get::<bool, _>("IMPORT_LOCAL_ARTWORK"),
        import_barcode_isbn: row.get::<bool, _>("IMPORT_BARCODE_ISBN"),
        scan_force_modified_time: row.get::<bool, _>("SCAN_FORCE_MODIFIED_TIME"),
        scan_interval: row.get::<String, _>("SCAN_INTERVAL"),
        scan_on_startup: row.get::<bool, _>("SCAN_STARTUP"),
        scan_cbx: row.get::<bool, _>("SCAN_CBX"),
        scan_pdf: row.get::<bool, _>("SCAN_PDF"),
        scan_epub: row.get::<bool, _>("SCAN_EPUB"),
        scan_directory_exclusions: vec![],
        repair_extensions: row.get::<bool, _>("REPAIR_EXTENSIONS"),
        convert_to_cbz: row.get::<bool, _>("CONVERT_TO_CBZ"),
        empty_trash_after_scan: row.get::<bool, _>("EMPTY_TRASH_AFTER_SCAN"),
        series_cover: row.get::<String, _>("SERIES_COVER"),
        hash_files: row.get::<bool, _>("HASH_FILES"),
        hash_pages: row.get::<bool, _>("HASH_PAGES"),
        hash_koreader: row.get::<bool, _>("HASH_KOREADER"),
        analyze_dimensions: row.get::<bool, _>("ANALYZE_DIMENSIONS"),
        oneshots_directory: row.get::<Option<String>, _>("ONESHOTS_DIRECTORY"),
        unavailable: row.get::<Option<String>, _>("UNAVAILABLE_DATE").is_some(),
    }
}

fn libraries_payload(libraries: Vec<PersistedLibrary>, is_admin: bool) -> Value {
    Value::Array(
        libraries
            .into_iter()
            .map(|library| library_payload(library, is_admin))
            .collect(),
    )
}

fn library_payload(library: PersistedLibrary, is_admin: bool) -> Value {
    let root = if is_admin {
        library.root
    } else {
        String::new()
    };
    json!({
        "id": library.id,
        "name": library.name,
        "root": root,
        "importComicInfoBook": library.import_comicinfo_book,
        "importComicInfoSeries": library.import_comicinfo_series,
        "importComicInfoCollection": library.import_comicinfo_collection,
        "importComicInfoReadList": library.import_comicinfo_readlist,
        "importComicInfoSeriesAppendVolume": library.import_comicinfo_series_append_volume,
        "importEpubBook": library.import_epub_book,
        "importEpubSeries": library.import_epub_series,
        "importMylarSeries": library.import_mylar_series,
        "importLocalArtwork": library.import_local_artwork,
        "importBarcodeIsbn": library.import_barcode_isbn,
        "scanForceModifiedTime": library.scan_force_modified_time,
        "scanInterval": library.scan_interval,
        "scanOnStartup": library.scan_on_startup,
        "scanCbx": library.scan_cbx,
        "scanPdf": library.scan_pdf,
        "scanEpub": library.scan_epub,
        "scanDirectoryExclusions": library.scan_directory_exclusions,
        "repairExtensions": library.repair_extensions,
        "convertToCbz": library.convert_to_cbz,
        "emptyTrashAfterScan": library.empty_trash_after_scan,
        "seriesCover": library.series_cover,
        "hashFiles": library.hash_files,
        "hashPages": library.hash_pages,
        "hashKoreader": library.hash_koreader,
        "analyzeDimensions": library.analyze_dimensions,
        "oneshotsDirectory": library.oneshots_directory,
        "unavailable": library.unavailable,
    })
}
