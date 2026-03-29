use super::*;

pub async fn load_persisted_authors(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    persisted_backend_load_persisted_authors(database_file, library_id).await
}

pub async fn load_persisted_author_names(
    database_file: &FsPath,
    search: &str,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_author_names(database_file, search).await
}

pub async fn load_persisted_author_roles(database_file: &FsPath) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_author_roles(database_file).await
}

pub async fn load_persisted_authors_by_scope(
    database_file: &FsPath,
    scope: &PersistedAuthorsScope,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    persisted_backend_load_persisted_authors_by_scope(database_file, scope).await
}

pub fn authors_v2_page_payload(
    authors: Vec<PersistedAuthorEntry>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    let total_elements = authors.len();
    let page_size = if unpaged {
        total_elements.max(1)
    } else {
        size.max(1)
    };
    let offset = if unpaged {
        0
    } else {
        page.saturating_mul(page_size)
    };

    let content = if unpaged {
        authors
    } else if offset >= total_elements {
        vec![]
    } else {
        authors.into_iter().skip(offset).take(page_size).collect()
    };

    let total_pages = if total_elements == 0 {
        0
    } else if unpaged {
        1
    } else {
        total_elements.div_ceil(page_size)
    };
    let number = if unpaged { 0 } else { page };
    let number_of_elements = content.len();
    let first = number == 0;
    let last = total_pages == 0 || number + 1 >= total_pages;

    json!({
        "content": content,
        "number": number,
        "size": page_size,
        "first": first,
        "last": last,
        "empty": number_of_elements == 0,
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
