use super::*;

macro_rules! scoped_string_loader {
    ($name:ident, $backend:ident) => {
        pub async fn $name(
            database_file: &FsPath,
            library_ids: Option<&[String]>,
            collection_id: Option<&str>,
        ) -> Result<Vec<String>, String> {
            $backend(database_file, library_ids, collection_id).await
        }
    };
}

scoped_string_loader!(
    load_persisted_genres,
    persisted_backend_load_persisted_genres
);
scoped_string_loader!(load_persisted_tags, persisted_backend_load_persisted_tags);
scoped_string_loader!(
    load_persisted_languages,
    persisted_backend_load_persisted_languages
);
scoped_string_loader!(
    load_persisted_publishers,
    persisted_backend_load_persisted_publishers
);
scoped_string_loader!(
    load_persisted_age_ratings,
    persisted_backend_load_persisted_age_ratings
);
scoped_string_loader!(
    load_persisted_sharing_labels,
    persisted_backend_load_persisted_sharing_labels
);
scoped_string_loader!(
    load_persisted_series_release_dates,
    persisted_backend_load_persisted_series_release_dates
);
scoped_string_loader!(
    load_persisted_series_tags,
    persisted_backend_load_persisted_series_tags
);
