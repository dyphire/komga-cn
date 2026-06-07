use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PageHashAction {
    DeleteManual,
    DeleteAuto,
    Ignore,
}

impl PageHashAction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "DELETE_MANUAL" => Some(Self::DeleteManual),
            "DELETE_AUTO" => Some(Self::DeleteAuto),
            "IGNORE" => Some(Self::Ignore),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::DeleteManual => "DELETE_MANUAL",
            Self::DeleteAuto => "DELETE_AUTO",
            Self::Ignore => "IGNORE",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageHashSortDirection {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashSort {
    pub property: String,
    pub direction: PageHashSortDirection,
}

impl PageHashSort {
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.split(',');
        let property = parts.next()?.trim();
        if property.is_empty() {
            return None;
        }
        let direction = match parts.next().unwrap_or("asc").trim() {
            value if value.eq_ignore_ascii_case("desc") => PageHashSortDirection::Desc,
            _ => PageHashSortDirection::Asc,
        };
        Some(Self {
            property: property.to_string(),
            direction,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashKnownQuery {
    pub page: u64,
    pub size: u64,
    pub actions: Vec<PageHashAction>,
    pub sorts: Vec<PageHashSort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashUnknownQuery {
    pub page: u64,
    pub size: u64,
    pub sorts: Vec<PageHashSort>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashMatchesQuery {
    pub hash: String,
    pub page: u64,
    pub size: u64,
    pub sorts: Vec<PageHashSort>,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashKnownEntry {
    pub hash: String,
    pub size: Option<i64>,
    pub action: PageHashAction,
    pub delete_count: i64,
    pub match_count: i64,
    pub created: String,
    pub last_modified: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashUnknownEntry {
    pub hash: String,
    pub size: Option<i64>,
    pub match_count: i64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashMatchEntry {
    pub book_id: String,
    pub url: String,
    pub page_number: i64,
    pub file_name: String,
    pub file_size: i64,
    pub media_type: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashPage<T> {
    pub content: Vec<T>,
    pub pageable: PageHashPageable,
    pub last: bool,
    pub total_elements: u64,
    pub total_pages: u64,
    pub first: bool,
    pub size: u64,
    pub number: u64,
    pub sort: PageHashSortState,
    pub number_of_elements: u64,
    pub empty: bool,
}

impl<T> PageHashPage<T> {
    pub fn new(page: u64, size: u64, total_elements: u64, content: Vec<T>, sorted: bool) -> Self {
        let size = size.max(1);
        let offset = page.saturating_mul(size);
        let total_pages = if total_elements == 0 {
            0
        } else {
            total_elements.div_ceil(size)
        };
        let number_of_elements = content.len() as u64;
        let sort = PageHashSortState::new(sorted);

        Self {
            content,
            pageable: PageHashPageable {
                page_number: page,
                page_size: size,
                sort,
                offset,
                paged: true,
                unpaged: false,
            },
            last: total_pages == 0 || page + 1 >= total_pages,
            total_elements,
            total_pages,
            first: page == 0,
            size,
            number: page,
            sort,
            number_of_elements,
            empty: number_of_elements == 0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageHashPageable {
    pub page_number: u64,
    pub page_size: u64,
    pub sort: PageHashSortState,
    pub offset: u64,
    pub paged: bool,
    pub unpaged: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct PageHashSortState {
    pub empty: bool,
    pub sorted: bool,
    pub unsorted: bool,
}

impl PageHashSortState {
    fn new(sorted: bool) -> Self {
        Self {
            empty: !sorted,
            sorted,
            unsorted: !sorted,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_hash_page_matches_spring_page_shape_flags() {
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

        assert_eq!(page.pageable.page_number, 1);
        assert_eq!(page.pageable.page_size, 2);
        assert_eq!(page.pageable.offset, 2);
        assert!(!page.first);
        assert!(!page.last);
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.number_of_elements, 1);
        assert!(!page.empty);
        assert!(page.sort.sorted);
        assert!(!page.sort.unsorted);
    }

    #[test]
    fn page_hash_sort_parser_preserves_unknown_properties_for_adapter_filtering() {
        assert_eq!(
            PageHashSort::parse("matchCount,desc"),
            Some(PageHashSort {
                property: "matchCount".to_string(),
                direction: PageHashSortDirection::Desc,
            }),
        );
        assert_eq!(
            PageHashSort::parse("unknown"),
            Some(PageHashSort {
                property: "unknown".to_string(),
                direction: PageHashSortDirection::Asc,
            }),
        );
        assert_eq!(PageHashSort::parse(""), None);
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

    #[test]
    fn page_hash_action_parser_is_exact() {
        assert_eq!(
            PageHashAction::parse("DELETE_MANUAL"),
            Some(PageHashAction::DeleteManual),
        );
        assert_eq!(PageHashAction::parse(" DELETE_MANUAL "), None);
    }
}
