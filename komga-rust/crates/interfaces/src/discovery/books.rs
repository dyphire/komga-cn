use super::filters::{exact_oneshot_bootstrap_series_id, normalize_release_date_date_time};
use super::persisted::common_helpers::{decode_query_component, filter_rows};
use super::persisted::delegates::{
    internal_error_response, invalid_runtime_books_list_response, load_persisted_book_tags,
    load_persisted_books_page, load_persisted_duplicate_books, load_persisted_ondeck_books,
    remap_requested_library_ids_for_persisted, requested_query_values,
    runtime_owned_books_latest_response, runtime_owned_books_list_response,
};
use super::persisted::models::{
    BooksFilterCriteria, PersistedBookTagsScope, PersistedBooksBrowseQuery, PersistedBooksSortMode,
};
use super::*;
use crate::discovery_auth::context::{
    DetailContentContext, DetailResourceContext, QueryRestrictions,
};
use crate::discovery_auth::principal::AgeRestrictionKind;
use crate::helpers::detail_access_denial_response;
use axum::extract::State;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
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

fn legacy_books_boolean_condition(kind: &str, value: bool) -> Value {
    json!({
        "type": kind,
        "operator": if value { "isTrue" } else { "isFalse" },
    })
}

fn legacy_books_any_of_condition(kind: &str, values: Vec<String>) -> Value {
    if values.len() == 1 {
        return json!({
            "type": kind,
            "operator": "is",
            "value": values.into_iter().next().unwrap_or_default(),
        });
    }

    json!({
        "type": "AnyOfBook",
        "conditions": values
            .into_iter()
            .map(|value| json!({
                "type": kind,
                "operator": "is",
                "value": value,
            }))
            .collect::<Vec<_>>(),
    })
}

fn legacy_books_query_condition(
    library_ids: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    read_statuses: Option<Vec<String>>,
    media_statuses: Option<Vec<String>>,
    released_after: Option<String>,
) -> Option<Value> {
    let mut conditions = Vec::new();

    if let Some(library_ids) = library_ids.filter(|values| !values.is_empty()) {
        conditions.push(legacy_books_any_of_condition("LibraryId", library_ids));
    }
    if let Some(tags) = tags.filter(|values| !values.is_empty()) {
        conditions.push(legacy_books_any_of_condition("Tag", tags));
    }
    if let Some(read_statuses) = read_statuses.filter(|values| !values.is_empty()) {
        conditions.push(legacy_books_any_of_condition("ReadStatus", read_statuses));
    }
    if let Some(media_statuses) = media_statuses.filter(|values| !values.is_empty()) {
        conditions.push(legacy_books_any_of_condition("MediaStatus", media_statuses));
    }
    if let Some(released_after) = released_after {
        conditions.push(json!({
            "type": "ReleaseDate",
            "operator": "after",
            "dateTime": released_after,
        }));
    }

    match conditions.len() {
        0 => None,
        1 => conditions.into_iter().next(),
        _ => Some(json!({
            "type": "AllOfBook",
            "conditions": conditions,
        })),
    }
}

fn empty_books_page_response(page: usize, size: usize, unpaged: bool, sorted: bool) -> Response {
    Json(books_page_payload(
        PageEnvelope {
            content: vec![],
            page,
            size,
            total_elements: 0,
            total_pages: 0,
        },
        false,
        !unpaged,
        sorted,
    ))
    .into_response()
}

fn legacy_series_books_payload(series_id: &str, uri: &Uri) -> Result<Value, StatusCode> {
    let query = uri.query().unwrap_or_default();
    let mut conditions = vec![json!({
        "type": "SeriesId",
        "operator": "is",
        "value": series_id,
    })];

    for (key, condition_type) in [
        ("tag", "Tag"),
        ("read_status", "ReadStatus"),
        ("media_status", "MediaStatus"),
        ("author", "Author"),
    ] {
        if let Some(values) = decoded_query_values(query, key) {
            conditions.push(legacy_books_any_of_condition(condition_type, values));
        }
    }

    let deleted = optional_query_bool(query, "deleted").map_err(|()| StatusCode::BAD_REQUEST)?;
    if let Some(deleted) = deleted {
        conditions.push(legacy_books_boolean_condition("Deleted", deleted));
    }

    Ok(json!({
        "condition": {
            "type": "AllOfBook",
            "conditions": conditions,
        }
    }))
}

fn legacy_series_books_uri(uri: &Uri) -> Result<Uri, StatusCode> {
    let query = uri.query().unwrap_or_default();
    if !query_values(query, "sort").is_empty() {
        return Ok(uri.clone());
    }

    let path_and_query = if query.is_empty() {
        format!("{}?sort=number,asc", uri.path())
    } else {
        format!("{}?{}&sort=number,asc", uri.path(), query)
    };

    path_and_query
        .parse::<Uri>()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

#[derive(Clone, Copy)]
enum DuplicateBooksSortField {
    Name,
    Series,
    Created,
    LastModified,
    FileSize,
    FileHash,
    Url,
    MediaStatus,
    MediaComment,
    MediaType,
    MediaPagesCount,
    MetadataTitle,
    MetadataNumberSort,
    MetadataReleaseDate,
    ReadProgressLastModified,
    ReadProgressReadDate,
}

#[derive(Clone, Copy)]
struct DuplicateBooksSortMode {
    field: DuplicateBooksSortField,
    descending: bool,
}

struct DuplicateBookPayload {
    payload: Value,
    series_title_sort: String,
}

struct DuplicateBooksPageSlice {
    content: Vec<Value>,
    page: usize,
    size: usize,
    total_elements: usize,
}

fn duplicate_books_unicode_collator() -> icu::collator::CollatorBorrowed<'static> {
    let mut options = CollatorOptions::default();
    options.strength = Some(Strength::Tertiary);
    Collator::try_new(locale!("und").into(), options)
        .expect("unicode collator for duplicate books sorting should construct")
}

fn compare_duplicate_book_unicode_strings(
    collator: &icu::collator::CollatorBorrowed<'_>,
    left: Option<&str>,
    right: Option<&str>,
    descending: bool,
) -> std::cmp::Ordering {
    let ordering = match (left, right) {
        (Some(left), Some(right)) => collator.compare(left, right),
        (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    };
    if descending {
        ordering.reverse()
    } else {
        ordering
    }
}

fn parse_duplicate_books_sort_modes(sorts: &[String]) -> Vec<DuplicateBooksSortMode> {
    sorts
        .iter()
        .filter_map(|sort| {
            let (field, direction) = sort.split_once(',').unwrap_or((sort.as_str(), "asc"));
            let field = match field {
                "name" => DuplicateBooksSortField::Name,
                "series" => DuplicateBooksSortField::Series,
                "created" | "createdDate" => DuplicateBooksSortField::Created,
                "lastModified" | "lastModifiedDate" => DuplicateBooksSortField::LastModified,
                "fileSize" | "size" => DuplicateBooksSortField::FileSize,
                "fileHash" => DuplicateBooksSortField::FileHash,
                "url" => DuplicateBooksSortField::Url,
                "media.status" => DuplicateBooksSortField::MediaStatus,
                "media.comment" => DuplicateBooksSortField::MediaComment,
                "media.mediaType" => DuplicateBooksSortField::MediaType,
                "media.pagesCount" => DuplicateBooksSortField::MediaPagesCount,
                "metadata.title" => DuplicateBooksSortField::MetadataTitle,
                "metadata.numberSort" => DuplicateBooksSortField::MetadataNumberSort,
                "metadata.releaseDate" => DuplicateBooksSortField::MetadataReleaseDate,
                "readProgress.lastModified" => DuplicateBooksSortField::ReadProgressLastModified,
                "readProgress.readDate" => DuplicateBooksSortField::ReadProgressReadDate,
                _ => return None,
            };
            Some(DuplicateBooksSortMode {
                field,
                descending: direction.eq_ignore_ascii_case("desc"),
            })
        })
        .collect()
}

fn duplicate_books_sort_modes(query: &str, unpaged: bool) -> Vec<DuplicateBooksSortMode> {
    if unpaged {
        return vec![];
    }

    let sort_values = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    if sort_values.is_empty() {
        vec![DuplicateBooksSortMode {
            field: DuplicateBooksSortField::FileHash,
            descending: false,
        }]
    } else {
        parse_duplicate_books_sort_modes(&sort_values)
    }
}

fn duplicate_books_sort_value(
    book: &DuplicateBookPayload,
    field: DuplicateBooksSortField,
) -> Option<&Value> {
    match field {
        DuplicateBooksSortField::Name => book.payload.get("name"),
        DuplicateBooksSortField::Series => None,
        DuplicateBooksSortField::Created => book.payload.get("created"),
        DuplicateBooksSortField::LastModified => book.payload.get("lastModified"),
        DuplicateBooksSortField::FileSize => book.payload.get("sizeBytes"),
        DuplicateBooksSortField::FileHash => book.payload.get("fileHash"),
        DuplicateBooksSortField::Url => book.payload.get("url"),
        DuplicateBooksSortField::MediaStatus => book.payload.pointer("/media/status"),
        DuplicateBooksSortField::MediaComment => book.payload.pointer("/media/comment"),
        DuplicateBooksSortField::MediaType => book.payload.pointer("/media/mediaType"),
        DuplicateBooksSortField::MediaPagesCount => book.payload.pointer("/media/pagesCount"),
        DuplicateBooksSortField::MetadataTitle => book.payload.pointer("/metadata/title"),
        DuplicateBooksSortField::MetadataNumberSort => book.payload.pointer("/metadata/numberSort"),
        DuplicateBooksSortField::MetadataReleaseDate => {
            book.payload.pointer("/metadata/releaseDate")
        }
        DuplicateBooksSortField::ReadProgressLastModified => {
            book.payload.pointer("/readProgress/lastModified")
        }
        DuplicateBooksSortField::ReadProgressReadDate => {
            book.payload.pointer("/readProgress/readDate")
        }
    }
}

fn compare_duplicate_book_payload_values(
    left: Option<&Value>,
    right: Option<&Value>,
    descending: bool,
) -> std::cmp::Ordering {
    let ordering = match (left, right) {
        (Some(Value::String(left)), Some(Value::String(right))) => {
            left.to_lowercase().cmp(&right.to_lowercase())
        }
        (Some(Value::Number(left)), Some(Value::Number(right))) => {
            match (left.as_f64(), right.as_f64()) {
                (Some(left), Some(right)) => left
                    .partial_cmp(&right)
                    .unwrap_or(std::cmp::Ordering::Equal),
                _ => std::cmp::Ordering::Equal,
            }
        }
        (Some(Value::Bool(left)), Some(Value::Bool(right))) => left.cmp(right),
        (Some(Value::Null), Some(Value::Null)) => std::cmp::Ordering::Equal,
        (Some(Value::Null), Some(_)) | (None, Some(_)) => std::cmp::Ordering::Less,
        (Some(_), Some(Value::Null)) | (Some(_), None) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
        _ => std::cmp::Ordering::Equal,
    };
    if descending {
        ordering.reverse()
    } else {
        ordering
    }
}

fn sort_duplicate_book_payloads(
    books: &mut [DuplicateBookPayload],
    sort_modes: &[DuplicateBooksSortMode],
) {
    let unicode_collator = duplicate_books_unicode_collator();
    books.sort_by(|left, right| {
        for sort_mode in sort_modes {
            let ordering = match sort_mode.field {
                DuplicateBooksSortField::Name => compare_duplicate_book_unicode_strings(
                    &unicode_collator,
                    left.payload.get("name").and_then(Value::as_str),
                    right.payload.get("name").and_then(Value::as_str),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::Series => compare_duplicate_book_unicode_strings(
                    &unicode_collator,
                    Some(left.series_title_sort.as_str()),
                    Some(right.series_title_sort.as_str()),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MetadataTitle => compare_duplicate_book_unicode_strings(
                    &unicode_collator,
                    left.payload
                        .pointer("/metadata/title")
                        .and_then(Value::as_str),
                    right
                        .payload
                        .pointer("/metadata/title")
                        .and_then(Value::as_str),
                    sort_mode.descending,
                ),
                _ => compare_duplicate_book_payload_values(
                    duplicate_books_sort_value(left, sort_mode.field),
                    duplicate_books_sort_value(right, sort_mode.field),
                    sort_mode.descending,
                ),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.payload
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .cmp(
                right
                    .payload
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default(),
            )
    });
}

fn duplicate_books_page_payload(
    content: Vec<Value>,
    page: usize,
    size: usize,
    total_elements: usize,
    sorted: bool,
) -> Value {
    let safe_size = size.max(1);
    let total_pages = if total_elements == 0 {
        0
    } else {
        ((total_elements - 1) / safe_size) + 1
    };
    let number_of_elements = content.len();
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;
    let sort = json!({
        "empty": !sorted,
        "sorted": sorted,
        "unsorted": !sorted,
    });

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": safe_size,
            "sort": sort.clone(),
            "offset": page.saturating_mul(safe_size),
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": safe_size,
        "number": page,
        "sort": sort,
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    })
}

fn slice_duplicate_books_page(
    books: Vec<DuplicateBookPayload>,
    requested_page: usize,
    requested_size: usize,
    unpaged: bool,
) -> DuplicateBooksPageSlice {
    let total_elements = books.len();

    if unpaged {
        return DuplicateBooksPageSlice {
            content: books.into_iter().map(|book| book.payload).collect(),
            page: 0,
            size: total_elements.max(20),
            total_elements,
        };
    }

    let offset = requested_page.saturating_mul(requested_size);
    let content = books
        .into_iter()
        .skip(offset)
        .take(requested_size)
        .map(|book| book.payload)
        .collect();

    DuplicateBooksPageSlice {
        content,
        page: requested_page,
        size: requested_size,
        total_elements,
    }
}

fn normalize_books_latest_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    const KOTLIN_PAGE_SIZE: usize = 20;

    page.page = 0;
    page.size = KOTLIN_PAGE_SIZE;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / KOTLIN_PAGE_SIZE) + 1
    };
    page
}

fn normalized_ondeck_sharing_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

fn ondeck_content_allowed_by_restrictions(
    restrictions: Option<&QueryRestrictions>,
    age_rating: Option<u16>,
    sharing_labels: &[String],
) -> bool {
    let Some(restrictions) = restrictions else {
        return true;
    };

    let labels = normalized_ondeck_sharing_labels(sharing_labels);

    let age_allowed = if restrictions.age_restriction == Some(AgeRestrictionKind::AllowOnly) {
        restrictions
            .age
            .map(|age_limit| age_rating.is_some_and(|age| age <= age_limit))
    } else {
        None
    };
    let label_allowed = if restrictions.labels_allow.is_empty() {
        None
    } else {
        Some(
            restrictions
                .labels_allow
                .iter()
                .any(|candidate| labels.contains(candidate)),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, label_allowed) => label_allowed != Some(false),
        (age_allowed, None) => age_allowed != Some(false),
        (age_allowed, label_allowed) => age_allowed != Some(false) || label_allowed != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = if restrictions.age_restriction == Some(AgeRestrictionKind::Exclude) {
        restrictions
            .age
            .is_some_and(|age_limit| age_rating.is_some_and(|age| age >= age_limit))
    } else {
        false
    };
    let label_denied = if restrictions.labels_exclude.is_empty() {
        false
    } else {
        restrictions
            .labels_exclude
            .iter()
            .any(|candidate| labels.contains(candidate))
    };

    !age_denied && !label_denied
}

fn ondeck_page_payload(content: Vec<Value>, uri: &Uri) -> Value {
    let query = uri.query().unwrap_or_default();
    let requested_page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let requested_size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let total_elements = content.len();
    let page_size = if unpaged {
        total_elements.max(20)
    } else {
        requested_size
    };
    let offset = if unpaged {
        0
    } else {
        requested_page.saturating_mul(page_size)
    };
    let content = if unpaged {
        content
    } else if offset >= total_elements {
        vec![]
    } else {
        content.into_iter().skip(offset).take(page_size).collect()
    };

    let page = if unpaged { 0 } else { requested_page };
    let total_pages = if total_elements == 0 {
        0
    } else {
        total_elements.div_ceil(page_size)
    };
    let number_of_elements = content.len();
    let first = page == 0;
    let last = total_pages == 0 || page + 1 >= total_pages;
    let sort = json!({
        "empty": true,
        "sorted": false,
        "unsorted": true,
    });

    json!({
        "content": content,
        "pageable": {
            "pageNumber": page,
            "pageSize": page_size,
            "sort": sort.clone(),
            "offset": offset,
            "paged": true,
            "unpaged": false,
        },
        "last": last,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "first": first,
        "size": page_size,
        "number": page,
        "sort": sort,
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0,
    })
}

fn should_use_strict_runtime_shape(payload: &Value, has_oneshot_bootstrap: bool) -> bool {
    has_oneshot_bootstrap
        || payload
            .get("condition")
            .and_then(|condition| condition.get("type"))
            .is_some()
}

pub async fn books_list(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    if !app.auth_db.db.database_file().exists() {
        return StatusCode::NOT_FOUND.into_response();
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
    let is_exact_oneshot_bootstrap = exact_oneshot_bootstrap_series_id(Some(&payload)).is_some();
    let strict_runtime_shape =
        should_use_strict_runtime_shape(&payload, is_exact_oneshot_bootstrap);

    if let Some(runtime_response) = runtime_owned_books_list_response(
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

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime books filter combination".to_string(),
    ))
}

pub(super) async fn books_deprecated_get(
    headers: HeaderMap,
    uri: Uri,
    app: &HttpAppState,
) -> Response {
    let database_file = app.auth_db.db.database_file();
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.services.discovery_persisted.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await;
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
            let Some(normalized) = normalize_release_date_date_time(&decoded) else {
                return StatusCode::BAD_REQUEST.into_response();
            };
            Some(normalized)
        }
        None => None,
    };

    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let search = requested_query_values(query, "search")
        .and_then(|values| values.into_iter().next())
        .filter(|value| !value.trim().is_empty());
    let sorted = !query_values(query, "sort").is_empty() || search.is_some();
    let requested_non_empty_library_ids = requested_library_ids
        .as_ref()
        .is_some_and(|values| !values.is_empty());
    if requested_non_empty_library_ids && library_ids.is_none() {
        return empty_books_page_response(page, size, unpaged, sorted);
    }

    let payload = legacy_books_query_condition(
        library_ids.clone(),
        tags.clone(),
        read_statuses.clone(),
        media_statuses.clone(),
        released_after.clone(),
    )
    .map(|condition| json!({ "condition": condition }));

    if let Some(runtime_response) = runtime_owned_books_list_response(
        app.services.discovery_persisted.as_ref(),
        &headers,
        &uri,
        payload.as_ref(),
        search.clone(),
        &app.discovery_auth,
        &*app.services.runtime_identity,
        true,
    )
    .await
    {
        return runtime_response;
    }

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported legacy books filter combination".to_string(),
    ))
}

pub async fn series_books_deprecated(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
    AxumPath(series_id): AxumPath<String>,
) -> Response {
    let database_file = app.auth_db.db.database_file();
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    if !database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let resolved_series_id = super::detail::resolve_series_id_for_persisted(&app, &series_id).await;
    let Some(resource) =
        (match super::detail::load_persisted_series_resource(&app, &resolved_series_id).await {
            Ok(resource) => resource,
            Err(error) => return internal_error_response(error),
        })
    else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let detail_context = DetailResourceContext {
        library_id: Some(resource.library_id),
        content: Some(DetailContentContext {
            age_rating: resource.age_rating,
            sharing_labels: resource.sharing_labels,
        }),
    };

    if let Err(denial) = app
        .discovery_auth
        .resolve_detail_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            &detail_context,
        )
        .await
    {
        return detail_access_denial_response(denial);
    }

    let payload = match legacy_series_books_payload(&resolved_series_id, &uri) {
        Ok(payload) => payload,
        Err(status) => return status.into_response(),
    };
    let uri = match legacy_series_books_uri(&uri) {
        Ok(uri) => uri,
        Err(status) => return status.into_response(),
    };

    if let Some(response) = runtime_owned_books_list_response(
        app.services.discovery_persisted.as_ref(),
        &headers,
        &uri,
        Some(&payload),
        None,
        &app.discovery_auth,
        &*app.services.runtime_identity,
        true,
    )
    .await
    {
        return response;
    }

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported legacy series books filter combination".to_string(),
    ))
}

pub async fn books_latest(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    let query = uri.query().unwrap_or_default();

    if app.auth_db.db.database_file().exists() {
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_requested_library_ids_for_persisted(
            app.services.discovery_persisted.as_ref(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

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

        let page = query_value(query, "page")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        let size = query_value(query, "size")
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(20)
            .max(1);
        let unpaged = query_bool(query, "unpaged");

        match load_persisted_books_page(
            app.services.discovery_persisted.as_ref(),
            &context,
            PersistedBooksBrowseQuery::from_filters(
                BooksFilterCriteria {
                    library_ids,
                    ..BooksFilterCriteria::default()
                },
                None,
                page,
                size,
                unpaged,
                vec![PersistedBooksSortMode::LastModifiedDateDesc],
            ),
        )
        .await
        {
            Ok(page) => {
                let (page, paged) = if unpaged {
                    (normalize_books_latest_unpaged_page_shape(page), true)
                } else {
                    (page, true)
                };
                return Json(books_page_payload(page, context.is_admin, paged, true))
                    .into_response();
            }
            Err(error) => return internal_error_response(error),
        }
    }

    if app.auth_db.db.database_file().exists()
        && let Some(runtime_response) = runtime_owned_books_latest_response(
            app.services.discovery_persisted.as_ref(),
            &headers,
            &uri,
            &app.discovery_auth,
        )
        .await
    {
        return runtime_response;
    }

    if !app.auth_db.db.database_file().exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime books latest filter combination".to_string(),
    ))
}

pub async fn books_ondeck(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    if !app.auth_db.db.database_file().exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let library_ids = remap_requested_library_ids_for_persisted(
        app.services.discovery_persisted.as_ref(),
        requested_library_ids.as_ref(),
    )
    .await
    .or(requested_library_ids);
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
    let Some(user_id) = context.user_id.as_deref() else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    match load_persisted_ondeck_books(app.services.discovery_persisted.as_ref(), user_id).await {
        Ok(entries) => {
            let filtered_entries =
                if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
                    filter_rows(entries, |row| {
                        allowed_ids.iter().any(|id| id == row.library_id.as_str())
                    })
                } else {
                    entries
                };
            let mut content = Vec::with_capacity(filtered_entries.len());
            for entry in filtered_entries {
                let resource =
                    match super::detail::load_persisted_book_resource(&app, &entry.id).await {
                        Ok(Some(resource)) => resource,
                        Ok(None) => {
                            return internal_error_response(format!(
                                "missing persisted on-deck book resource for '{}'",
                                entry.id
                            ));
                        }
                        Err(error) => return internal_error_response(error),
                    };

                if !ondeck_content_allowed_by_restrictions(
                    context.restrictions.as_ref(),
                    resource.age_rating,
                    &resource.sharing_labels,
                ) {
                    continue;
                }

                let detail =
                    match super::detail::load_persisted_book_detail(&app, &entry.id, Some(user_id))
                        .await
                    {
                        Ok(Some(detail)) => detail,
                        Ok(None) => {
                            return internal_error_response(format!(
                                "missing persisted on-deck book detail for '{}'",
                                entry.id
                            ));
                        }
                        Err(error) => return internal_error_response(error),
                    };
                content.push(super::detail::book_detail_payload(
                    &detail,
                    context.is_admin,
                ));
            }

            let mut response = Json(ondeck_page_payload(content, &uri)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn books_duplicates(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_request_admin(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    if !app.auth_db.db.database_file().exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_request_auth_user(&*app.services.runtime_identity, &headers).await
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();
    let query = uri.query().unwrap_or_default();
    let requested_page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let requested_size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");
    let sort_modes = duplicate_books_sort_modes(query, unpaged);

    match load_persisted_duplicate_books(app.services.discovery_persisted.as_ref()).await {
        Ok(entries) => {
            let mut content = Vec::with_capacity(entries.len());
            for entry in entries {
                let detail = match super::detail::load_persisted_book_detail(
                    &app,
                    &entry.id,
                    Some(&user_id),
                )
                .await
                {
                    Ok(Some(detail)) => detail,
                    Ok(None) => {
                        return internal_error_response(format!(
                            "missing persisted duplicate book detail for '{}'",
                            entry.id
                        ));
                    }
                    Err(error) => return internal_error_response(error),
                };
                content.push(DuplicateBookPayload {
                    payload: super::detail::book_detail_payload(&detail, true),
                    series_title_sort: detail.series_title_sort,
                });
            }

            if !sort_modes.is_empty() {
                sort_duplicate_book_payloads(&mut content, &sort_modes);
            }

            let page_slice =
                slice_duplicate_books_page(content, requested_page, requested_size, unpaged);

            let mut response = Json(duplicate_books_page_payload(
                page_slice.content,
                page_slice.page,
                page_slice.size,
                page_slice.total_elements,
                !sort_modes.is_empty(),
            ))
            .into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn book_tags(
    State(app): State<Arc<HttpAppState>>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_request_auth(&*app.services.runtime_identity, &headers).await {
        return response;
    }

    if !app.auth_db.db.database_file().exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let query = uri.query().unwrap_or_default();
    let library_ids = query_values(query, "library_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let series_scope = query_value(query, "series_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let readlist_scope = query_value(query, "readlist_id")
        .filter(|value| !value.is_empty())
        .map(decode_query_component);
    let context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &*app.services.runtime_identity,
            &headers,
            if series_scope.is_some() || readlist_scope.is_some() || library_ids.is_empty() {
                None
            } else {
                Some(library_ids.as_slice())
            },
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let scope = series_scope
        .map(PersistedBookTagsScope::Series)
        .or_else(|| readlist_scope.map(PersistedBookTagsScope::ReadList))
        .or_else(|| {
            context
                .authorized_library_ids
                .clone()
                .filter(|ids| !ids.is_empty())
                .map(PersistedBookTagsScope::Libraries)
        })
        .or(Some(PersistedBookTagsScope::All));

    match load_persisted_book_tags(
        app.services.discovery_persisted.as_ref(),
        scope.as_ref(),
        context.authorized_library_ids.as_deref(),
    )
    .await
    {
        Ok(tags) => Json(json!(tags)).into_response(),
        Err(error) => internal_error_response(error),
    }
}
