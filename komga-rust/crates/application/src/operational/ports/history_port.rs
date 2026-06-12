use async_trait::async_trait;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistorySortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistorySortProperty {
    Type,
    BookId,
    SeriesId,
    Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistorySort {
    pub property: HistorySortProperty,
    pub direction: HistorySortDirection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistorySortSelection {
    pub sorts: Vec<HistorySort>,
    pub sorted: bool,
}

impl HistorySortSelection {
    pub fn default_timestamp_desc() -> Self {
        Self {
            sorts: vec![HistorySort {
                property: HistorySortProperty::Timestamp,
                direction: HistorySortDirection::Desc,
            }],
            sorted: true,
        }
    }

    pub fn from_requested_sorts(sorts: Vec<HistorySort>) -> Self {
        Self {
            sorted: !sorts.is_empty(),
            sorts,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryPage {
    pub content: Vec<HistoryEvent>,
    pub page: u64,
    pub size: u64,
    pub total_elements: u64,
    pub total_pages: u64,
    pub sorted: bool,
}

impl HistoryPage {
    pub fn new(
        page: u64,
        size: u64,
        total_elements: u64,
        content: Vec<HistoryEvent>,
        sorted: bool,
    ) -> Self {
        let size = size.max(1);
        let total_pages = if total_elements == 0 {
            0
        } else {
            total_elements.div_ceil(size)
        };

        Self {
            content,
            page,
            size,
            total_elements,
            total_pages,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryEvent {
    pub id: String,
    pub event_type: String,
    pub book_id: Option<String>,
    pub series_id: Option<String>,
    pub timestamp: String,
    pub properties: BTreeMap<String, String>,
}

#[async_trait]
pub trait HistoryPort: Send + Sync {
    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sort: HistorySortSelection,
    ) -> Result<HistoryPage, String>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_page_keeps_only_pagination_facts() {
        let page = HistoryPage::new(1, 2, 5, Vec::new(), true);

        assert_eq!(page.page, 1);
        assert_eq!(page.size, 2);
        assert_eq!(page.offset(), 2);
        assert_eq!(page.total_pages, 3);
        assert!(page.sorted);
    }
}
