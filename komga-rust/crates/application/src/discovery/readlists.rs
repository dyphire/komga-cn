use komga_domain::discovery::{BookReadModel, DiscoveryError, DiscoveryQueryContext, PageEnvelope};

use super::core::{DiscoveryQueries, DiscoveryQueryRepository};
use super::helpers::unsupported_book_filter;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadListBooksQuery {
    pub readlist_id: String,
    pub page: usize,
    pub size: usize,
    pub unpaged: bool,
    pub library_ids: Option<Vec<String>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeReadListBooksQuery {
    pub readlist_id: String,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub async fn list_readlist_books(
        &self,
        context: &DiscoveryQueryContext,
        query: ReadListBooksQuery,
    ) -> Result<PageEnvelope<BookReadModel>, DiscoveryError> {
        if !query.unpaged {
            return Err(unsupported_book_filter("paged"));
        }

        if query.library_ids.is_some() {
            return Err(unsupported_book_filter("LibraryId"));
        }

        self.repository.list_readlist_books(
            context,
            NativeReadListBooksQuery {
                readlist_id: query.readlist_id,
            },
        )
        .await
    }
}
