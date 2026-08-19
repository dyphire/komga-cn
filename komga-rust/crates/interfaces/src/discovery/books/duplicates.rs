use axum::Json;
use axum::extract::State;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use icu::collator::{
    Collator,
    options::{CollatorOptions, Strength},
};
use icu::locale::locale;

use crate::contracts::common::PageDto;
use crate::contracts::discovery::BookDto;
use crate::helpers::{query_bool, query_value, query_values};
use crate::identity_access::auth::Admin;
use crate::state::DiscoveryState;
use komga_application::identity_access::user_id;

use super::super::persisted::common_helpers::{decode_query_component, internal_error_response};

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

struct DuplicateBooksSortRequest<'a> {
    field: &'a str,
    descending: bool,
}

impl<'a> DuplicateBooksSortRequest<'a> {
    fn parse(sort: &'a str) -> Self {
        match sort.split_once(',') {
            Some((field, direction)) => Self {
                field,
                descending: direction.eq_ignore_ascii_case("desc"),
            },
            None => Self {
                field: sort,
                descending: false,
            },
        }
    }
}

struct DuplicateBookPayload {
    payload: BookDto,
    series_title_sort: String,
}

struct DuplicateBooksPageSlice {
    content: Vec<BookDto>,
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
            let requested_sort = DuplicateBooksSortRequest::parse(sort);
            let field = match requested_sort.field {
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
                descending: requested_sort.descending,
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

fn compare_duplicate_book_strings(left: &str, right: &str, descending: bool) -> std::cmp::Ordering {
    let ordering = left.to_lowercase().cmp(&right.to_lowercase());
    if descending {
        ordering.reverse()
    } else {
        ordering
    }
}

fn compare_duplicate_book_numbers<T: PartialOrd>(
    left: T,
    right: T,
    descending: bool,
) -> std::cmp::Ordering {
    let ordering = left
        .partial_cmp(&right)
        .unwrap_or(std::cmp::Ordering::Equal);
    if descending {
        ordering.reverse()
    } else {
        ordering
    }
}

fn compare_duplicate_book_options<T: Ord>(
    left: Option<&T>,
    right: Option<&T>,
    descending: bool,
) -> std::cmp::Ordering {
    let ordering = left.cmp(&right);
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
                    Some(left.payload.name.as_str()),
                    Some(right.payload.name.as_str()),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::Series => compare_duplicate_book_unicode_strings(
                    &unicode_collator,
                    Some(left.series_title_sort.as_str()),
                    Some(right.series_title_sort.as_str()),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::Created => compare_duplicate_book_options(
                    Some(&left.payload.created),
                    Some(&right.payload.created),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::LastModified => compare_duplicate_book_options(
                    Some(&left.payload.last_modified),
                    Some(&right.payload.last_modified),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::FileSize => compare_duplicate_book_numbers(
                    left.payload.size_bytes,
                    right.payload.size_bytes,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::FileHash => compare_duplicate_book_strings(
                    &left.payload.file_hash,
                    &right.payload.file_hash,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::Url => compare_duplicate_book_strings(
                    &left.payload.url,
                    &right.payload.url,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MediaStatus => compare_duplicate_book_strings(
                    &left.payload.media.status,
                    &right.payload.media.status,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MediaComment => compare_duplicate_book_strings(
                    &left.payload.media.comment,
                    &right.payload.media.comment,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MediaType => compare_duplicate_book_strings(
                    &left.payload.media.media_type,
                    &right.payload.media.media_type,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MediaPagesCount => compare_duplicate_book_numbers(
                    left.payload.media.pages_count,
                    right.payload.media.pages_count,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MetadataTitle => compare_duplicate_book_unicode_strings(
                    &unicode_collator,
                    Some(left.payload.metadata.title.as_str()),
                    Some(right.payload.metadata.title.as_str()),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MetadataNumberSort => compare_duplicate_book_numbers(
                    left.payload.metadata.number_sort,
                    right.payload.metadata.number_sort,
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::MetadataReleaseDate => compare_duplicate_book_options(
                    left.payload.metadata.release_date.as_ref(),
                    right.payload.metadata.release_date.as_ref(),
                    sort_mode.descending,
                ),
                DuplicateBooksSortField::ReadProgressLastModified => {
                    compare_duplicate_book_options(
                        left.payload
                            .read_progress
                            .as_ref()
                            .map(|progress| &progress.last_modified),
                        right
                            .payload
                            .read_progress
                            .as_ref()
                            .map(|progress| &progress.last_modified),
                        sort_mode.descending,
                    )
                }
                DuplicateBooksSortField::ReadProgressReadDate => compare_duplicate_book_options(
                    left.payload
                        .read_progress
                        .as_ref()
                        .map(|progress| &progress.read_date),
                    right
                        .payload
                        .read_progress
                        .as_ref()
                        .map(|progress| &progress.read_date),
                    sort_mode.descending,
                ),
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.payload.id.cmp(&right.payload.id)
    });
}

fn duplicate_books_page_payload(
    content: Vec<BookDto>,
    page: usize,
    size: usize,
    total_elements: usize,
    sorted: bool,
) -> PageDto<BookDto> {
    let safe_size = size.max(1);
    let total_pages = if total_elements == 0 {
        0
    } else {
        ((total_elements - 1) / safe_size) + 1
    };

    PageDto::from_parts(
        content,
        page,
        safe_size,
        total_elements,
        total_pages,
        true,
        sorted,
    )
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

pub(crate) async fn books_duplicates(
    State(app): State<DiscoveryState>,
    admin: Admin,
    uri: Uri,
) -> Response {
    let user_id = user_id(&admin).to_string();
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

    match app.book_special_lists.load_duplicate_books().await {
        Ok(entries) => {
            let mut content = Vec::with_capacity(entries.len());
            for entry in entries {
                let detail = match super::super::detail::load_persisted_book_detail(
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
                let payload = match BookDto::from_read_model(&detail, true) {
                    Ok(payload) => payload,
                    Err(error) => return internal_error_response(error),
                };
                content.push(DuplicateBookPayload {
                    payload,
                    series_title_sort: detail.series_title_sort,
                });
            }

            if !sort_modes.is_empty() {
                sort_duplicate_book_payloads(&mut content, &sort_modes);
            }

            let page_slice =
                slice_duplicate_books_page(content, requested_page, requested_size, unpaged);

            Json(duplicate_books_page_payload(
                page_slice.content,
                page_slice.page,
                page_slice.size,
                page_slice.total_elements,
                !sort_modes.is_empty(),
            ))
            .into_response()
        }
        Err(error) => internal_error_response(error),
    }
}
