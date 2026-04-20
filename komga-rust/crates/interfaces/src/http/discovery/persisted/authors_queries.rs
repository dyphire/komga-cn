use crate::discovery_persisted_access::PersistedDiscoveryService;

use super::common_helpers::{PagePayloadMetadata, page_payload};
use super::*;

pub async fn load_persisted_author_names(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
    search: &str,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    backend
        .load_persisted_author_names(
            database_file.to_path_buf(),
            search.to_string(),
            authorized_library_ids.map(|ids| ids.to_vec()),
        )
        .await
}

pub async fn load_persisted_author_roles(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    backend
        .load_persisted_author_roles(
            database_file.to_path_buf(),
            authorized_library_ids.map(|ids| ids.to_vec()),
        )
        .await
}

pub async fn load_persisted_authors_by_scope(
    backend: &dyn PersistedDiscoveryService,
    database_file: &FsPath,
    scope: &PersistedAuthorsScope,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<PersistedAuthorEntry>, String> {
    backend
        .load_persisted_authors_by_scope(
            database_file.to_path_buf(),
            scope.clone(),
            authorized_library_ids.map(|ids| ids.to_vec()),
        )
        .await
}

pub fn authors_v2_page_payload(
    authors: Vec<PersistedAuthorEntry>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    let total_elements = authors.len();
    let page_size = if unpaged {
        total_elements.max(20)
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

    page_payload(
        content.into_iter().map(|author| json!(author)).collect(),
        PagePayloadMetadata {
            page: if unpaged { 0 } else { page },
            size: page_size,
            total_elements,
            total_pages,
            paged: true,
            sorted: true,
            offset: if unpaged { 0 } else { offset },
        },
    )
}
