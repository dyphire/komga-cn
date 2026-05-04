use komga_domain::discovery::DiscoveryError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeReadListsQuery {
    pub page: usize,
    pub size: usize,
    pub library_ids: Option<Vec<String>>,
    pub search: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListBooksQuery {
    pub readlist_id: String,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
    pub deleted: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub read_statuses: Option<Vec<String>>,
    pub media_statuses: Option<Vec<String>>,
    pub authors: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListDetailQuery {
    pub readlist_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadListBooksOwnership {
    RuntimeOwned,
    DependencyOnly,
}

pub fn classify_readlist_books_query(
    query: &ReadListBooksQuery,
) -> Result<ReadListBooksOwnership, DiscoveryError> {
    if !query.unpaged {
        return Ok(ReadListBooksOwnership::RuntimeOwned);
    }

    Ok(ReadListBooksOwnership::DependencyOnly)
}

pub fn normalize_readlists_search(search: Option<String>) -> Option<String> {
    search.and_then(|value| (!value.trim().is_empty()).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::{
        ReadListBooksOwnership, ReadListBooksQuery, classify_readlist_books_query,
        normalize_readlists_search,
    };

    #[test]
    fn normalize_readlists_search_returns_none_for_blank_effective_values() {
        assert_eq!(normalize_readlists_search(None), None);
        assert_eq!(normalize_readlists_search(Some(String::new())), None);
        assert_eq!(
            normalize_readlists_search(Some("   \t\n".to_string())),
            None
        );
    }

    #[test]
    fn normalize_readlists_search_preserves_non_blank_value_without_trimming() {
        let decoded = " alpha ".to_string();

        assert_eq!(
            normalize_readlists_search(Some(decoded.clone())),
            Some(decoded),
        );
    }

    #[test]
    fn classify_readlist_books_query_accepts_unpaged_with_library_and_extra_filters() {
        let query = ReadListBooksQuery {
            readlist_id: "readlist-1".to_string(),
            page: 0,
            size: 20,
            unpaged: true,
            library_ids: Some(vec!["library-1".to_string()]),
            deleted: Some(false),
            tags: Some(vec!["favorite".to_string()]),
            read_statuses: Some(vec!["read".to_string()]),
            media_statuses: Some(vec!["READY".to_string()]),
            authors: Some(vec!["alice".to_string()]),
        };

        assert_eq!(
            classify_readlist_books_query(&query),
            Ok(ReadListBooksOwnership::DependencyOnly),
        );
    }
}
