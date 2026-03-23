use komga_domain::discovery::{
    BookReadModel, DiscoveryError, DiscoveryQueryContext, PageEnvelope, ReadListReadModel,
};

use super::core::{DiscoveryQueries, DiscoveryQueryRepository};
use super::helpers::{unsupported_book_filter, unsupported_book_sort};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListsQuery {
    pub page: usize,
    pub size: usize,
    pub library_ids: Option<Vec<String>>,
    pub search: Option<String>,
    pub unpaged: bool,
    pub sort: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeReadListsQuery {
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
pub struct NativeReadListBooksQuery {
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
    NativeOwned,
    DependencyOnly,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub async fn get_readlist_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListDetailQuery,
    ) -> Result<Option<ReadListReadModel>, DiscoveryError> {
        self.repository.get_readlist_detail(context, query).await
    }

    pub async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        classify_readlist_books_query(&query)?;
        self.repository
            .list_readlist_books(context, native_readlist_books_query(query))
            .await
    }
}

pub fn classify_readlist_books_query(
    query: &ReadListBooksQuery,
) -> Result<ReadListBooksOwnership, DiscoveryError> {
    if !query.unpaged {
        return Ok(ReadListBooksOwnership::NativeOwned);
    }

    if query.library_ids.is_some() {
        return Err(unsupported_book_filter("LibraryId"));
    }

    let has_extra_filters = query.deleted.is_some()
        || query.tags.is_some()
        || query.read_statuses.is_some()
        || query.media_statuses.is_some()
        || query.authors.is_some();
    if has_extra_filters {
        return Err(unsupported_book_filter("extra-filters"));
    }

    Ok(ReadListBooksOwnership::DependencyOnly)
}

pub fn classify_readlists_browse_query(query: &ReadListsQuery) -> Result<(), DiscoveryError> {
    if query.unpaged {
        return Err(unsupported_book_filter("unpaged"));
    }

    if let Some(sort) = query.sort.first() {
        return Err(unsupported_book_sort(sort.clone()));
    }

    Ok(())
}

pub fn normalize_readlists_search(search: Option<String>) -> Option<String> {
    search.and_then(|value| (!value.trim().is_empty()).then_some(value))
}

#[cfg(test)]
mod tests {
    use super::normalize_readlists_search;

    #[test]
    fn normalize_readlists_search_returns_none_for_blank_effective_values() {
        assert_eq!(normalize_readlists_search(None), None);
        assert_eq!(normalize_readlists_search(Some(String::new())), None);
        assert_eq!(normalize_readlists_search(Some("   \t\n".to_string())), None);
    }

    #[test]
    fn normalize_readlists_search_preserves_non_blank_value_without_trimming() {
        let decoded = " alpha ".to_string();

        assert_eq!(
            normalize_readlists_search(Some(decoded.clone())),
            Some(decoded),
        );
    }
}

pub(in crate::discovery) fn native_readlist_books_query(
    query: ReadListBooksQuery,
) -> NativeReadListBooksQuery {
    NativeReadListBooksQuery {
        readlist_id: query.readlist_id,
        page: query.page,
        size: query.size,
        unpaged: query.unpaged,
        library_ids: query.library_ids,
        deleted: query.deleted,
        tags: query.tags,
        read_statuses: query.read_statuses,
        media_statuses: query.media_statuses,
        authors: query.authors,
    }
}
