#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageHashAction {
    DeleteManual,
    DeleteAuto,
    Ignore,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageHashSortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageHashKnownSortProperty {
    Hash,
    MatchCount,
    DeleteCount,
    DeleteSize,
    FileSize,
    CreatedDate,
    LastModifiedDate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageHashUnknownSortProperty {
    Hash,
    FileSize,
    MatchCount,
    TotalSize,
    Url,
    BookId,
    PageNumber,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageHashMatchSortProperty {
    Hash,
    FileSize,
    Url,
    BookId,
    PageNumber,
    MatchCount,
    TotalSize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageHashSort<P> {
    pub property: P,
    pub direction: PageHashSortDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashKnownQuery {
    pub page: u64,
    pub size: u64,
    pub actions: Vec<PageHashAction>,
    pub sorts: Vec<PageHashSort<PageHashKnownSortProperty>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashUnknownQuery {
    pub page: u64,
    pub size: u64,
    pub sorts: Vec<PageHashSort<PageHashUnknownSortProperty>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashMatchesQuery {
    pub hash: String,
    pub page: u64,
    pub size: u64,
    pub sorts: Vec<PageHashSort<PageHashMatchSortProperty>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashUpsertCommand {
    pub hash: String,
    pub size: Option<i64>,
    pub action: PageHashAction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PageHashCommandError {
    BlankHash,
}

impl PageHashUpsertCommand {
    pub fn new(
        hash: String,
        size: Option<i64>,
        action: PageHashAction,
    ) -> Result<Self, PageHashCommandError> {
        if hash.trim().is_empty() {
            return Err(PageHashCommandError::BlankHash);
        }

        Ok(Self {
            hash,
            size: size.filter(|value| *value >= 0),
            action,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashThumbnail {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashDeleteTargetPage {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashDeleteTarget {
    pub book_id: String,
    pub pages: Vec<PageHashDeleteTargetPage>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashKnownEntry {
    pub hash: String,
    pub size: Option<i64>,
    pub action: PageHashAction,
    pub delete_count: i64,
    pub match_count: i64,
    pub created: String,
    pub last_modified: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashUnknownEntry {
    pub hash: String,
    pub size: Option<i64>,
    pub match_count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashMatchEntry {
    pub book_id: String,
    pub url: String,
    pub page_number: i64,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashPage<T> {
    pub content: Vec<T>,
    pub total_elements: u64,
    pub total_pages: u64,
    pub page: u64,
    pub size: u64,
    pub sorted: bool,
}

impl<T> PageHashPage<T> {
    pub fn new(page: u64, size: u64, total_elements: u64, content: Vec<T>, sorted: bool) -> Self {
        let size = size.max(1);
        let total_pages = if total_elements == 0 {
            0
        } else {
            total_elements.div_ceil(size)
        };

        Self {
            content,
            total_elements,
            total_pages,
            page,
            size,
            sorted,
        }
    }

    pub fn offset(&self) -> u64 {
        self.page.saturating_mul(self.size)
    }

    pub fn number_of_elements(&self) -> u64 {
        self.content.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_hash_page_keeps_only_pagination_facts() {
        let page = PageHashPage::new(
            1,
            2,
            5,
            vec![PageHashUnknownEntry {
                hash: "hash-1".to_string(),
                size: Some(10),
                match_count: 2,
            }],
            true,
        );

        assert_eq!(page.page, 1);
        assert_eq!(page.size, 2);
        assert_eq!(page.offset(), 2);
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.content.len(), 1);
        assert!(page.sorted);
    }

    #[test]
    fn page_hash_upsert_command_preserves_hash_and_normalizes_negative_size() {
        let command = PageHashUpsertCommand::new(
            " padded-hash ".to_string(),
            Some(-1),
            PageHashAction::Ignore,
        )
        .expect("padded hash should be accepted");

        assert_eq!(command.hash, " padded-hash ");
        assert_eq!(command.size, None);
        assert_eq!(command.action, PageHashAction::Ignore);
    }

    #[test]
    fn page_hash_upsert_command_rejects_blank_hash() {
        assert_eq!(
            PageHashUpsertCommand::new("   ".to_string(), Some(1), PageHashAction::Ignore),
            Err(PageHashCommandError::BlankHash),
        );
    }
}
