use super::*;

use crate::discovery_persisted_access::PersistedDiscoveryService;

macro_rules! scoped_string_loader {
    ($name:ident, $field:ident) => {
        pub async fn $name(
            backend: &dyn PersistedDiscoveryService,
            database_file: &FsPath,
            library_ids: Option<&[String]>,
            collection_id: Option<&str>,
        ) -> Result<Vec<String>, String> {
            backend
                .$field(
                    database_file.to_path_buf(),
                    library_ids.map(|ids| ids.to_vec()),
                    collection_id.map(str::to_string),
                )
                .await
        }
    };
}

scoped_string_loader!(load_persisted_genres, load_persisted_genres);
scoped_string_loader!(load_persisted_tags, load_persisted_tags);
scoped_string_loader!(load_persisted_languages, load_persisted_languages);
scoped_string_loader!(load_persisted_publishers, load_persisted_publishers);
scoped_string_loader!(load_persisted_age_ratings, load_persisted_age_ratings);
scoped_string_loader!(load_persisted_sharing_labels, load_persisted_sharing_labels);
scoped_string_loader!(
    load_persisted_series_release_dates,
    load_persisted_series_release_dates
);
scoped_string_loader!(load_persisted_series_tags, load_persisted_series_tags);
