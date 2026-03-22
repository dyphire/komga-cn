use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext, LibraryReadModel};

use super::core::{DiscoveryQueries, DiscoveryQueryRepository};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LibraryListQuery {}

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
}
