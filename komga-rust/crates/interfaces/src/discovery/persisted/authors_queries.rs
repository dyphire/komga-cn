use crate::contracts::common::PageDto;

pub(in crate::discovery) fn paged_values_payload<T>(
    values: Vec<T>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> PageDto<T> {
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

    PageDto::from_parts(
        content,
        if unpaged { 0 } else { page },
        page_size,
        total_elements,
        total_pages,
        true,
        true,
    )
}
