use super::*;
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;
use komga_domain::discovery::PageEnvelope;

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
            let (field, direction) = sort
                .split_once(',')
                .map(|(field, direction)| (field, direction))
                .unwrap_or((sort.as_str(), "asc"));
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

fn duplicate_books_sort_value<'a>(
    book: &'a DuplicateBookPayload,
    field: DuplicateBooksSortField,
) -> Option<&'a Value> {
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

fn should_use_strict_runtime_shape(payload: Option<&Value>, has_oneshot_bootstrap: bool) -> bool {
    has_oneshot_bootstrap
        || payload
            .and_then(|value| value.get("condition"))
            .and_then(|condition| condition.get("type"))
            .is_some()
}

pub async fn books(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    if auth_db.database_file.exists() {
        let query = uri.query().unwrap_or_default();
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_requested_library_ids_for_persisted(
            auth_db.database_file.as_path(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

        let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
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
        let sort_values = query_values(query, "sort")
            .into_iter()
            .map(decode_query_component)
            .collect::<Vec<_>>();
        let search = query_value(query, "search").map(decode_query_component);

        let sort_modes = parse_persisted_books_sort_modes(&sort_values);
        match load_persisted_books_page(
            auth_db.database_file.as_path(),
            &context,
            PersistedBooksBrowseQuery::from_filters(
                BooksFilterCriteria {
                    library_ids,
                    ..BooksFilterCriteria::default()
                },
                search,
                page,
                size,
                unpaged,
                sort_modes,
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
                return Json(books_page_payload(page, context.is_admin, paged)).into_response();
            }
            Err(error) => return internal_error_response(error),
        }
    }

    empty_books_page_response(&uri, false)
}

pub async fn books_list(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let payload = serde_json::from_slice::<Value>(&body).ok();
    if payload.as_ref().is_some_and(contains_legacy_search_input) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let full_text_search = payload.as_ref().and_then(extract_full_text_search);
    let is_exact_oneshot_bootstrap = exact_oneshot_bootstrap_series_id(payload.as_ref()).is_some();
    let strict_runtime_shape =
        should_use_strict_runtime_shape(payload.as_ref(), is_exact_oneshot_bootstrap);

    if (auth_db.database_file.exists() || is_exact_oneshot_bootstrap)
        && let Some(runtime_response) = runtime_owned_books_list_response(
            &headers,
            &uri,
            payload.as_ref(),
            full_text_search.clone(),
            &auth_state,
            auth_db.database_file.as_path(),
            strict_runtime_shape,
        )
        .await
    {
        return runtime_response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime books filter combination".to_string(),
    ))
}

pub async fn books_latest(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();

    if auth_db.database_file.exists() {
        let requested_library_ids = requested_query_values(query, "library_id");
        let library_ids = remap_requested_library_ids_for_persisted(
            auth_db.database_file.as_path(),
            requested_library_ids.as_ref(),
        )
        .await
        .or(requested_library_ids);

        let context = match auth_state.resolve_query_context(&headers, library_ids.as_deref()) {
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
            auth_db.database_file.as_path(),
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
                return Json(books_page_payload(page, context.is_admin, paged)).into_response();
            }
            Err(error) => return internal_error_response(error),
        }
    }

    if auth_db.database_file.exists()
        && let Some(runtime_response) = runtime_owned_books_latest_response(
            &headers,
            &uri,
            &auth_state,
            auth_db.database_file.as_path(),
        )
        .await
    {
        return runtime_response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    invalid_runtime_books_list_response(DiscoveryError::InvalidSemantics(
        "unsupported runtime books latest filter combination".to_string(),
    ))
}

pub async fn books_ondeck(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let user_id = user_id(&user).to_string();

    match load_persisted_ondeck_books(auth_db.database_file.as_path(), &user_id).await {
        Ok(entries) => {
            let mut response = Json(books_page_for_entries(entries, &uri)).into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(error) => internal_error_response(error),
    }
}

pub async fn books_duplicates(
    Extension(auth_db): Extension<AuthDatabaseState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
        return StatusCode::NOT_FOUND.into_response();
    }

    let Some(user) = resolved_auth_user(&headers) else {
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

    match load_persisted_duplicate_books(auth_db.database_file.as_path()).await {
        Ok(entries) => {
            let mut content = Vec::with_capacity(entries.len());
            for entry in entries {
                let detail = match super::detail::load_persisted_book_detail(
                    auth_db.database_file.as_path(),
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
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    if !auth_db.database_file.exists() {
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
    let context = match auth_state.resolve_query_context(
        &headers,
        if series_scope.is_some() || readlist_scope.is_some() || library_ids.is_empty() {
            None
        } else {
            Some(library_ids.as_slice())
        },
    ) {
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
        auth_db.database_file.as_path(),
        scope.as_ref(),
        context.authorized_library_ids.as_deref(),
    )
    .await
    {
        Ok(tags) => Json(json!(tags)).into_response(),
        Err(error) => internal_error_response(error),
    }
}
