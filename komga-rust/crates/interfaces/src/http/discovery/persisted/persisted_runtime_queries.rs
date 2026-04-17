use super::*;

macro_rules! persisted_runtime_loader {
    ($name:ident, $backend:ident, () -> $ret:ty) => {
        pub async fn $name(database_file: &FsPath) -> Result<$ret, String> {
            $backend(database_file).await
        }
    };
    ($name:ident, $backend:ident, ($arg:ident : $arg_ty:ty) -> $ret:ty) => {
        pub async fn $name(database_file: &FsPath, $arg: $arg_ty) -> Result<$ret, String> {
            $backend(database_file, $arg).await
        }
    };
    ($name:ident, $backend:ident, ($first:ident : $first_ty:ty, $second:ident : $second_ty:ty) -> $ret:ty) => {
        pub async fn $name(
            database_file: &FsPath,
            $first: $first_ty,
            $second: $second_ty,
        ) -> Result<$ret, String> {
            $backend(database_file, $first, $second).await
        }
    };
}

persisted_runtime_loader!(
    load_persisted_ondeck_books,
    persisted_backend_load_persisted_ondeck_books,
    (user_id: &str) -> Vec<PersistedBookBrowseEntry>
);
persisted_runtime_loader!(
    load_persisted_duplicate_books,
    persisted_backend_load_persisted_duplicate_books,
    () -> Vec<PersistedBookBrowseEntry>
);
persisted_runtime_loader!(
    load_persisted_book_tags,
    persisted_backend_load_persisted_book_tags,
    (
        scope: Option<&PersistedBookTagsScope>,
        authorized_library_ids: Option<&[String]>
    ) -> Vec<String>
);
persisted_runtime_loader!(
    persisted_utc_date_minus_days,
    persisted_backend_persisted_utc_date_minus_days,
    (days: i64) -> Option<String>
);
persisted_runtime_loader!(
    load_series_read_progress_counts,
    persisted_backend_load_series_read_progress_counts,
    (user_id: &str) -> HashMap<String, (i64, i64)>
);
persisted_runtime_loader!(
    load_series_read_dates,
    persisted_backend_load_series_read_dates,
    (user_id: &str) -> HashMap<String, String>
);
persisted_runtime_loader!(
    load_series_total_book_counts,
    persisted_backend_load_series_total_book_counts,
    () -> HashMap<String, i64>
);
