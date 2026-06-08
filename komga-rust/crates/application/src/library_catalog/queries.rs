use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

use super::{LibraryCatalogReadPort, LibraryRecord};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryDetailAccess {
    Visible(LibraryRecord),
    Forbidden,
    NotFound,
}

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

    pub async fn library_detail_access(
        &self,
        context: &DiscoveryQueryContext,
        library_id: &str,
    ) -> Result<LibraryDetailAccess, DiscoveryError> {
        if let Some(library) = self.port.get_library(context, library_id).await? {
            return Ok(LibraryDetailAccess::Visible(library));
        }

        if self
            .port
            .get_library(&unrestricted_query_context(), library_id)
            .await?
            .is_some()
        {
            Ok(LibraryDetailAccess::Forbidden)
        } else {
            Ok(LibraryDetailAccess::NotFound)
        }
    }
}

fn unrestricted_query_context() -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: None,
        is_admin: true,
        authorized_library_ids: None,
        restrictions: None,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use komga_domain::common_ids::{LibraryId, UserId};
    use komga_domain::discovery::{DiscoveryError, DiscoveryQueryContext};

    use super::{LibraryCatalogQueryService, LibraryDetailAccess};
    use crate::library_catalog::{LibraryCatalogReadPort, LibraryRecord};

    #[tokio::test]
    async fn library_detail_access_returns_forbidden_for_existing_restricted_library() {
        let service =
            LibraryCatalogQueryService::new(TestLibraryPort::with_libraries([library_record(
                "library-a",
            )]));

        let access = service
            .library_detail_access(&context_with_libraries(["library-b"]), "library-a")
            .await
            .expect("library detail access should resolve");

        assert_eq!(access, LibraryDetailAccess::Forbidden);
    }

    #[tokio::test]
    async fn library_detail_access_returns_visible_library_for_allowed_context() {
        let library = library_record("library-a");
        let service =
            LibraryCatalogQueryService::new(TestLibraryPort::with_libraries([library.clone()]));

        let access = service
            .library_detail_access(&context_with_libraries(["library-a"]), "library-a")
            .await
            .expect("library detail access should resolve");

        assert_eq!(access, LibraryDetailAccess::Visible(library));
    }

    #[tokio::test]
    async fn library_detail_access_returns_not_found_for_missing_library() {
        let service =
            LibraryCatalogQueryService::new(TestLibraryPort::with_libraries([library_record(
                "library-a",
            )]));

        let access = service
            .library_detail_access(&context_with_libraries(["library-b"]), "library-missing")
            .await
            .expect("library detail access should resolve");

        assert_eq!(access, LibraryDetailAccess::NotFound);
    }

    struct TestLibraryPort {
        libraries: Vec<LibraryRecord>,
    }

    impl TestLibraryPort {
        fn with_libraries<const N: usize>(libraries: [LibraryRecord; N]) -> Self {
            Self {
                libraries: libraries.into_iter().collect(),
            }
        }
    }

    #[async_trait]
    impl LibraryCatalogReadPort for TestLibraryPort {
        async fn list_libraries(
            &self,
            context: &DiscoveryQueryContext,
        ) -> Result<Vec<LibraryRecord>, DiscoveryError> {
            Ok(self
                .libraries
                .iter()
                .filter(|library| context_allows_library(context, library.id.as_str()))
                .cloned()
                .collect())
        }

        async fn get_library(
            &self,
            context: &DiscoveryQueryContext,
            library_id: &str,
        ) -> Result<Option<LibraryRecord>, DiscoveryError> {
            if !context_allows_library(context, library_id) {
                return Ok(None);
            }
            Ok(self
                .libraries
                .iter()
                .find(|library| library.id == library_id)
                .cloned())
        }
    }

    fn context_allows_library(context: &DiscoveryQueryContext, library_id: &str) -> bool {
        context
            .authorized_library_ids
            .as_ref()
            .is_none_or(|ids| ids.iter().any(|id| id.as_str() == library_id))
    }

    fn context_with_libraries<const N: usize>(library_ids: [&str; N]) -> DiscoveryQueryContext {
        DiscoveryQueryContext {
            user_id: Some(UserId::from("user-1")),
            is_admin: false,
            authorized_library_ids: Some(library_ids.into_iter().map(LibraryId::from).collect()),
            restrictions: None,
        }
    }

    fn library_record(id: &str) -> LibraryRecord {
        LibraryRecord {
            id: id.to_string(),
            ..LibraryRecord::default_record(id.to_string())
        }
    }
}
