use super::*;

pub(crate) fn books_page_for_entries(entries: Vec<PersistedBookBrowseEntry>, uri: &Uri) -> Value {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let requested_size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

    let total_elements = entries.len();
    let page_size = if unpaged {
        total_elements.max(1)
    } else {
        requested_size
    };
    let offset = if unpaged {
        0
    } else {
        page.saturating_mul(page_size)
    };

    let page_entries = if unpaged {
        entries
    } else if offset >= total_elements {
        vec![]
    } else {
        entries.into_iter().skip(offset).take(page_size).collect()
    };

    let content = page_entries
        .into_iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "libraryId": entry.library_id,
                "name": entry.name,
                "metadata": {
                    "title": entry.title,
                },
            })
        })
        .collect::<Vec<_>>();

    let number_of_elements = content.len();
    let total_pages = if total_elements == 0 {
        0
    } else if unpaged {
        1
    } else {
        total_elements.div_ceil(page_size)
    };
    let number = if unpaged { 0 } else { page };
    let first = number == 0;
    let last = total_pages == 0 || number + 1 >= total_pages;
    let empty = number_of_elements == 0;

    json!({
        "content": content,
        "number": number,
        "size": page_size,
        "first": first,
        "last": last,
        "empty": empty,
        "numberOfElements": number_of_elements,
        "totalElements": total_elements,
        "totalPages": total_pages,
        "sort": {
            "empty": true,
            "sorted": false,
            "unsorted": true,
        },
        "pageable": {
            "pageNumber": number,
            "pageSize": page_size,
            "offset": if unpaged { 0 } else { offset },
            "sort": {
                "empty": true,
                "sorted": false,
                "unsorted": true,
            },
            "paged": !unpaged,
            "unpaged": unpaged,
        },
    })
}

pub(crate) fn media_profile_for_media_type(media_type: &str) -> &'static str {
    match media_type {
        "application/zip"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => "divina",
        "application/epub+zip" => "epub",
        "application/pdf" => "pdf",
        _ => "",
    }
}

pub(crate) fn series_matches_read_status(
    row: &PersistedSeriesSummary,
    read_progress: Option<(i64, i64)>,
    status: &str,
) -> bool {
    match status.to_ascii_lowercase().as_str() {
        "unread" => read_progress.is_none(),
        "read" => read_progress
            .map(|(read_count, _)| read_count.max(0) as u64 == row.books_count)
            .unwrap_or(false),
        "in_progress" | "inprogress" => read_progress
            .map(|(read_count, _)| read_count.max(0) as u64 != row.books_count)
            .unwrap_or(false),
        _ => false,
    }
}

pub(crate) fn poster_matches(
    poster: &PersistedBookPosterSummary,
    poster_types: Option<&Vec<String>>,
    poster_selected: Option<bool>,
) -> bool {
    let type_matches = poster_types
        .map(|values| {
            values
                .iter()
                .any(|value| value.eq_ignore_ascii_case(&poster.thumbnail_type))
        })
        .unwrap_or(true);
    let selected_matches = poster_selected
        .map(|value| poster.selected == value)
        .unwrap_or(true);
    type_matches && selected_matches
}

pub(crate) fn author_value_matches(author: &str, expected: &str) -> bool {
    if let Some((expected_name, expected_role)) = expected.split_once("::") {
        let (author_name, author_role) = author
            .split_once("::")
            .map(|(name, role)| (name, Some(role)))
            .unwrap_or((author, None));

        if expected_name.is_empty() {
            return author_role
                .map(|role| role.eq_ignore_ascii_case(expected_role))
                .unwrap_or(false);
        }

        if expected_role.is_empty() {
            return author_name.eq_ignore_ascii_case(expected_name);
        }

        return author_name.eq_ignore_ascii_case(expected_name)
            && author_role
                .map(|role| role.eq_ignore_ascii_case(expected_role))
                .unwrap_or(false);
    }

    author.contains(expected)
}
