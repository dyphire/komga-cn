use super::common_helpers::{PagePayloadMetadata, page_payload};
use komga_application::discovery::PersistedAuthorEntry;
use serde_json::{Value, json};

pub(in crate::discovery) fn authors_v2_page_payload(
    authors: Vec<PersistedAuthorEntry>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    paged_values_payload(
        authors.into_iter().map(|author| json!(author)).collect(),
        page,
        size,
        unpaged,
    )
}

pub(in crate::discovery) fn paged_values_payload(
    values: Vec<Value>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Value {
    let total_elements = values.len();
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
        values
    } else if offset >= total_elements {
        vec![]
    } else {
        values.into_iter().skip(offset).take(page_size).collect()
    };

    let total_pages = if total_elements == 0 {
        0
    } else if unpaged {
        1
    } else {
        total_elements.div_ceil(page_size)
    };

    page_payload(
        content,
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
