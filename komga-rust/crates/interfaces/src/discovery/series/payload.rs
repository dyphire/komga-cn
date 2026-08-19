use crate::contracts::common::PageDto;
use crate::contracts::discovery::SeriesDto;
use komga_application::discovery::SeriesReadModel;
use komga_domain::discovery::PageEnvelope;

pub(in crate::discovery) fn series_read_model_page_payload(
    page: PageEnvelope<SeriesReadModel>,
    paged: bool,
    sorted: bool,
    kotlin_unpaged_shape: bool,
    is_admin: bool,
) -> anyhow::Result<PageDto<SeriesDto>> {
    let (page_number, page_size, total_pages) = if kotlin_unpaged_shape {
        let page_size = page.total_elements.max(20);
        let total_pages = if page.total_elements == 0 {
            0
        } else {
            ((page.total_elements - 1) / page_size) + 1
        };
        (0, page_size, total_pages)
    } else {
        (page.page, page.size, page.total_pages)
    };
    let content = page
        .content
        .iter()
        .map(|series| SeriesDto::from_read_model(series, is_admin))
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(PageDto::from_parts(
        content,
        page_number,
        page_size,
        page.total_elements,
        total_pages,
        paged,
        sorted,
    ))
}
