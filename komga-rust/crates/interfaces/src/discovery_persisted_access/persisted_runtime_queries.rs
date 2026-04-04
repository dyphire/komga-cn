use super::*;

pub async fn load_persisted_ondeck_books(
    database_file: &FsPath,
    user_id: &str,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    persisted_backend_load_persisted_ondeck_books(database_file, user_id).await
}

pub async fn load_persisted_duplicate_books(
    database_file: &FsPath,
) -> Result<Vec<PersistedBookBrowseEntry>, String> {
    persisted_backend_load_persisted_duplicate_books(database_file).await
}

pub async fn load_persisted_book_tags(
    database_file: &FsPath,
    scope: Option<&PersistedBookTagsScope>,
    authorized_library_ids: Option<&[String]>,
) -> Result<Vec<String>, String> {
    persisted_backend_load_persisted_book_tags(database_file, scope, authorized_library_ids).await
}

pub async fn persisted_utc_date_minus_days(
    database_file: &FsPath,
    days: i64,
) -> Result<Option<String>, String> {
    persisted_backend_persisted_utc_date_minus_days(database_file, days).await
}

pub async fn load_series_read_progress_counts(
    database_file: &FsPath,
    user_id: &str,
) -> Result<HashMap<String, (i64, i64)>, String> {
    persisted_backend_load_series_read_progress_counts(database_file, user_id).await
}

pub async fn load_series_total_book_counts(
    database_file: &FsPath,
) -> Result<HashMap<String, i64>, String> {
    persisted_backend_load_series_total_book_counts(database_file).await
}
