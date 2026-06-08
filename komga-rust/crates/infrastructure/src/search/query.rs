use std::path::PathBuf;

use super::index_dirs::resolve_discovery_index_dir;
use super::index_lifecycle::{SearchEntityType, SearchQueryLifecycle};

#[derive(Clone, Debug)]
pub struct SearchIndexQuery {
    database_file: PathBuf,
    default_index_dir: PathBuf,
}

impl SearchIndexQuery {
    pub fn new(database_file: PathBuf, default_index_dir: PathBuf) -> Self {
        Self {
            database_file,
            default_index_dir,
        }
    }

    pub fn search_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Vec<String> {
        let Ok(index) = SearchQueryLifecycle::bootstrap(self.index_dir().as_path()) else {
            return Vec::new();
        };

        index
            .search_ids(query, entity_type, limit)
            .unwrap_or_default()
    }

    pub fn search_scored_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Vec<(f32, String)> {
        let Ok(index) = SearchQueryLifecycle::bootstrap(self.index_dir().as_path()) else {
            return Vec::new();
        };

        index
            .search_scored_ids(query, entity_type, limit)
            .unwrap_or_default()
    }

    fn index_dir(&self) -> PathBuf {
        resolve_discovery_index_dir(&self.database_file, &self.default_index_dir)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::SearchIndexQuery;
    use crate::search::index_dirs::register_discovery_index_dir;
    use crate::search::index_lifecycle::{
        SearchDocument, SearchEntityType, SearchFieldEntry, SearchIndexLifecycle,
        SearchQueryLifecycle,
    };

    #[test]
    fn missing_index_searches_return_empty_without_creating_index_state() {
        let database_file = temp_path("missing-index-query", "sqlite");
        let index_dir = temp_path("missing-index-query", "index");
        let _ = std::fs::remove_dir_all(&index_dir);

        let query = SearchIndexQuery::new(database_file, index_dir.clone());

        assert_eq!(
            query.search_ids("anything", SearchEntityType::Book, 10),
            Vec::<String>::new()
        );
        assert_eq!(
            query.search_scored_ids("anything", SearchEntityType::Book, 10),
            Vec::<(f32, String)>::new()
        );
        assert!(
            !index_dir.exists(),
            "read-only search boundary must not create missing index directories"
        );
    }

    #[test]
    fn registered_index_searches_preserve_lifecycle_ordering() {
        let database_file = temp_path("registered-index-query", "sqlite");
        let default_index_dir = temp_path("registered-index-query-default", "index");
        let registered_index_dir = temp_path("registered-index-query-registered", "index");
        let _ = std::fs::remove_dir_all(&default_index_dir);
        let _ = std::fs::remove_dir_all(&registered_index_dir);
        register_discovery_index_dir(&database_file, &registered_index_dir);

        let index = SearchIndexLifecycle::bootstrap(registered_index_dir.as_path())
            .expect("registered index fixture should bootstrap");
        index
            .rebuild(&[
                collection_document("collection-1", "Alpha Shelf"),
                collection_document("collection-2", "Alpha Alpha Rack"),
            ])
            .expect("registered index fixture should rebuild");

        let expected = SearchQueryLifecycle::bootstrap(registered_index_dir.as_path())
            .expect("registered index query lifecycle should bootstrap")
            .search_scored_ids("Alpha", SearchEntityType::Collection, 10)
            .expect("registered index direct query should succeed");
        let actual = SearchIndexQuery::new(database_file, default_index_dir.clone())
            .search_scored_ids("Alpha", SearchEntityType::Collection, 10);

        assert_eq!(actual, expected);
        assert!(
            !default_index_dir.exists(),
            "registered mapping should avoid probing or creating the default index dir"
        );

        let _ = index.shutdown();
        let _ = std::fs::remove_dir_all(default_index_dir);
        let _ = std::fs::remove_dir_all(registered_index_dir);
    }

    fn collection_document(id: &str, name: &str) -> SearchDocument {
        SearchDocument {
            entity_type: SearchEntityType::Collection,
            id: id.to_string(),
            title: name.to_string(),
            fields: vec![SearchFieldEntry {
                field: "name".to_string(),
                value: name.to_string(),
            }],
        }
    }

    fn temp_path(case: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "komga-rust-search-query-{case}-{}-{nanos}.{extension}",
            std::process::id(),
        ))
    }
}
