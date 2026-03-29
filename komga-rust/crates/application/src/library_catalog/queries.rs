use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

use super::{LibraryCatalogReadPort, LibraryRecord};

pub struct LibraryCatalogQueryService<P> {
    port: P,
}

impl<P> LibraryCatalogQueryService<P>
where
    P: LibraryCatalogReadPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
    }

    pub async fn list_libraries(
        &self,
        context: &DiscoveryQueryContext,
    ) -> Result<Vec<LibraryRecord>, DiscoveryError> {
        self.port.list_libraries(context).await
    }

    pub async fn get_library(
        &self,
        context: &DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<Option<LibraryRecord>, DiscoveryError> {
        self.port.get_library(context, library_id).await
    }
}
