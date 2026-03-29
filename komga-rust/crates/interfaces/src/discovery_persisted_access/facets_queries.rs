use super::*;

pub async fn load_persisted_genres(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_genres(database_file, library_id).await
}

pub async fn load_persisted_tags(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_tags(database_file, library_id).await
}

pub async fn load_persisted_languages(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_languages(database_file, library_id).await
}

pub async fn load_persisted_publishers(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_publishers(database_file, library_id).await
}

pub async fn load_persisted_age_ratings(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<u16>, String> {
    persisted_backend_load_persisted_age_ratings(database_file, library_id).await
}

pub async fn load_persisted_sharing_labels(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_sharing_labels(database_file, library_id).await
}

pub async fn load_persisted_series_release_dates(
    database_file: &FsPath,
    library_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_series_release_dates(database_file, library_id).await
}

pub async fn load_persisted_series_tags(
    database_file: &FsPath,
    library_id: Option<&str>,
    collection_id: Option<&str>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_series_tags(database_file, library_id, collection_id).await
}
