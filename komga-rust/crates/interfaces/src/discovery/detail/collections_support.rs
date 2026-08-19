use crate::contracts::common::PageDto;
use crate::contracts::discovery::CollectionDto;
use komga_application::discovery::CollectionReadModel;
use komga_domain::discovery::PageEnvelope;

pub(super) fn collections_page_payload(
    page: PageEnvelope<CollectionReadModel>,
) -> anyhow::Result<PageDto<CollectionDto>> {
    let content = page
        .content
        .iter()
        .map(CollectionDto::from_read_model)
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(PageDto::paged(
        content,
        page.page,
        page.size,
        page.total_elements,
        page.total_pages,
        true,
    ))
}

pub(super) fn collections_unpaged_payload(
    content: Vec<CollectionReadModel>,
) -> anyhow::Result<PageDto<CollectionDto>> {
    let content = content
        .iter()
        .map(CollectionDto::from_read_model)
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(PageDto::unpaged(content, true))
}
