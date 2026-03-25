use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, LibraryReadModel};

use super::core::{DiscoveryQueries, DiscoveryQueryRepository};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryListQuery {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryDetailQuery {
    pub library_id: String,
}

impl<R> DiscoveryQueries<R>
where
    R: DiscoveryQueryRepository,
{
    pub async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
        _query: LibraryListQuery,
    ) -> Result<Vec<LibraryReadModel>, DiscoveryError> {
        self.repository.list_libraries(context).await
    }

    pub async fn get_library_detail(
        &self,
        context: &DiscoveryQueryContext,
        query: LibraryDetailQuery,
    ) -> Result<Option<LibraryReadModel>, DiscoveryError> {
        let libraries = self.repository.list_libraries(context).await?;
        Ok(libraries
            .into_iter()
            .find(|library| library.id == query.library_id))
    }
}
