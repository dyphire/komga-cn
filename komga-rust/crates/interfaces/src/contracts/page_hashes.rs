use anyhow::{Context, Result};
use komga_application::operational::{
    PageHashAction, PageHashKnownEntry, PageHashMatchEntry, PageHashPage, PageHashUnknownEntry,
};
use serde::Serialize;

use super::common::{KotlinLocalDateTime, PageDto};

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PageHashActionDto {
    DeleteManual,
    DeleteAuto,
    Ignore,
}

impl From<PageHashAction> for PageHashActionDto {
    fn from(action: PageHashAction) -> Self {
        match action {
            PageHashAction::DeleteManual => Self::DeleteManual,
            PageHashAction::DeleteAuto => Self::DeleteAuto,
            PageHashAction::Ignore => Self::Ignore,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashKnownDto {
    pub hash: String,
    pub size: Option<i64>,
    pub action: PageHashActionDto,
    pub delete_count: i32,
    pub match_count: i32,
    pub created: KotlinLocalDateTime,
    pub last_modified: KotlinLocalDateTime,
}

impl TryFrom<&PageHashKnownEntry> for PageHashKnownDto {
    type Error = anyhow::Error;

    fn try_from(entry: &PageHashKnownEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: entry.hash.clone(),
            size: entry.size,
            action: entry.action.into(),
            delete_count: i32::try_from(entry.delete_count)
                .context("page hash delete count exceeds Kotlin Int")?,
            match_count: i32::try_from(entry.match_count)
                .context("page hash match count exceeds Kotlin Int")?,
            created: KotlinLocalDateTime::parse(&entry.created)
                .context("parse page hash created timestamp")?,
            last_modified: KotlinLocalDateTime::parse(&entry.last_modified)
                .context("parse page hash last-modified timestamp")?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashUnknownDto {
    pub hash: String,
    pub size: Option<i64>,
    pub match_count: i32,
}

impl TryFrom<&PageHashUnknownEntry> for PageHashUnknownDto {
    type Error = anyhow::Error;

    fn try_from(entry: &PageHashUnknownEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            hash: entry.hash.clone(),
            size: entry.size,
            match_count: i32::try_from(entry.match_count)
                .context("page hash match count exceeds Kotlin Int")?,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashMatchDto {
    pub book_id: String,
    pub url: String,
    pub page_number: i32,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
}

impl TryFrom<&PageHashMatchEntry> for PageHashMatchDto {
    type Error = anyhow::Error;

    fn try_from(entry: &PageHashMatchEntry) -> Result<Self, Self::Error> {
        Ok(Self {
            book_id: entry.book_id.clone(),
            url: entry.url.clone(),
            page_number: i32::try_from(entry.page_number)
                .context("page hash page number exceeds Kotlin Int")?,
            file_name: entry.file_name.clone(),
            file_size: entry.file_size,
            media_type: entry.media_type.clone(),
        })
    }
}

pub fn known_page_hash_page(
    page: &PageHashPage<PageHashKnownEntry>,
) -> Result<PageDto<PageHashKnownDto>> {
    let content = page
        .content
        .iter()
        .map(PageHashKnownDto::try_from)
        .collect::<Result<Vec<_>>>()?;
    page_dto(page, content)
}

pub fn unknown_page_hash_page(
    page: &PageHashPage<PageHashUnknownEntry>,
) -> Result<PageDto<PageHashUnknownDto>> {
    let content = page
        .content
        .iter()
        .map(PageHashUnknownDto::try_from)
        .collect::<Result<Vec<_>>>()?;
    page_dto(page, content)
}

pub fn page_hash_matches_page(
    page: &PageHashPage<PageHashMatchEntry>,
) -> Result<PageDto<PageHashMatchDto>> {
    let content = page
        .content
        .iter()
        .map(PageHashMatchDto::try_from)
        .collect::<Result<Vec<_>>>()?;
    page_dto(page, content)
}

fn page_dto<C>(page: &PageHashPage<impl Sized>, content: Vec<C>) -> Result<PageDto<C>> {
    Ok(PageDto::paged(
        content,
        usize::try_from(page.page).context("page hash page number exceeds usize")?,
        usize::try_from(page.size).context("page hash page size exceeds usize")?,
        usize::try_from(page.total_elements).context("page hash total elements exceeds usize")?,
        usize::try_from(page.total_pages).context("page hash total pages exceeds usize")?,
        page.sorted,
    ))
}
