use super::{LibraryCatalogMutationError, LibraryCatalogMutationPort};

pub struct DeleteLibraryService<P> {
    port: P,
}

impl<P> DeleteLibraryService<P>
where
    P: LibraryCatalogMutationPort,
{
    pub fn new(port: P) -> Self {
        Self { port }
    }

    pub async fn delete_library(
        &self,
        library_id: &str,
    ) -> Result<bool, LibraryCatalogMutationError> {
        self.port
            .delete_library(library_id)
            .await
            .map_err(LibraryCatalogMutationError::persistence)
    }
}
