use super::*;

#[async_trait]
pub trait OperationalRuntimeService: Send + Sync {
    async fn load_task_execution_values(
        &self,
        tasks_db_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_libraries_count(&self, database_file: PathBuf) -> Result<f64, String>;
    async fn load_series_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_books_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_books_filesize_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_sidecars_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String>;
    async fn load_collections_count(&self, database_file: PathBuf) -> Result<f64, String>;
    async fn load_readlists_count(&self, database_file: PathBuf) -> Result<f64, String>;
    async fn load_task_failure_count(&self, database_file: PathBuf) -> Result<f64, String>;
    async fn load_sqlite_pool_snapshots(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<SqlitePoolSnapshot>, String>;
}

#[async_trait]
impl<T> OperationalRuntimeService for Arc<T>
where
    T: OperationalRuntimeService + ?Sized,
{
    async fn load_task_execution_values(
        &self,
        tasks_db_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        (**self).load_task_execution_values(tasks_db_file).await
    }

    async fn load_libraries_count(&self, database_file: PathBuf) -> Result<f64, String> {
        (**self).load_libraries_count(database_file).await
    }

    async fn load_series_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        (**self).load_series_grouped_by_library(database_file).await
    }

    async fn load_books_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        (**self).load_books_grouped_by_library(database_file).await
    }

    async fn load_books_filesize_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        (**self)
            .load_books_filesize_grouped_by_library(database_file)
            .await
    }

    async fn load_sidecars_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        (**self)
            .load_sidecars_grouped_by_library(database_file)
            .await
    }

    async fn load_collections_count(&self, database_file: PathBuf) -> Result<f64, String> {
        (**self).load_collections_count(database_file).await
    }

    async fn load_readlists_count(&self, database_file: PathBuf) -> Result<f64, String> {
        (**self).load_readlists_count(database_file).await
    }

    async fn load_task_failure_count(&self, database_file: PathBuf) -> Result<f64, String> {
        (**self).load_task_failure_count(database_file).await
    }

    async fn load_sqlite_pool_snapshots(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<SqlitePoolSnapshot>, String> {
        (**self).load_sqlite_pool_snapshots(paths).await
    }
}

#[async_trait]
pub trait OperationalSettingsService: Send + Sync {
    async fn load_announcement_read_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<String>, sqlx::Error>;
    async fn save_announcements_read(
        &self,
        database_file: PathBuf,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    async fn load_claim_status(&self, database_file: PathBuf) -> Result<bool, sqlx::Error>;
    async fn claim_initial_admin_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        email: String,
        password_hash: String,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error>;
    async fn load_client_settings_global(
        &self,
        database_file: PathBuf,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error>;
    async fn load_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Value, sqlx::Error>;
    async fn upsert_client_settings_global(
        &self,
        database_file: PathBuf,
        settings: Vec<(String, String, bool)>,
    ) -> Result<(), sqlx::Error>;
    async fn upsert_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        settings: Vec<(String, String)>,
    ) -> Result<(), sqlx::Error>;
    async fn delete_client_settings_global(
        &self,
        database_file: PathBuf,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    async fn delete_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    fn list_directory_entries(&self, path: PathBuf, directories_only: bool) -> Vec<Value>;
    fn list_font_families(&self, path: PathBuf) -> Vec<String>;
    fn load_font_family_css(&self, path: PathBuf, family: String) -> Option<String>;
    fn load_font_file(&self, path: PathBuf, family: String, file: String) -> Option<Vec<u8>>;
    async fn delete_syncpoints_by_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<(), sqlx::Error>;
    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
        key_ids: Vec<String>,
    ) -> Result<(), sqlx::Error>;
    async fn load_history_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_matches_page(
        &self,
        database_file: PathBuf,
        page_hash: String,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error>;
    async fn load_unknown_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error>;
    async fn load_page_hashes_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        actions: Vec<String>,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hashes_unknown_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error>;
    async fn load_page_hash_delete_targets(
        &self,
        database_file: PathBuf,
        hash: String,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error>;
    async fn upsert_page_hash(
        &self,
        database_file: PathBuf,
        hash: String,
        size: Option<i64>,
        action: String,
    ) -> Result<(), sqlx::Error>;
    fn analyze_transient_book(&self, path: String) -> TransientBookAnalysis;
    async fn infer_transient_series_and_number(
        &self,
        database_file: PathBuf,
        transient_name: String,
    ) -> (Option<String>, Option<f64>);
    fn list_transient_book_entries(&self, root: PathBuf) -> Vec<Value>;
    async fn validate_transient_scan_root(
        &self,
        database_file: PathBuf,
        path: String,
    ) -> Result<(), String>;
    fn load_transient_book_file_metadata(&self, path: String) -> Option<TransientBookFileMetadata>;
    fn load_transient_book_media(&self, path: String) -> Option<Vec<u8>>;
    fn transient_book_content_type(&self, path: String, media_type: String) -> &'static str;
    fn transient_book_page_content(
        &self,
        path: String,
        media_type: String,
        pages: Vec<TransientBookPage>,
        page_number: u32,
    ) -> Option<(String, Vec<u8>)>;
}

#[async_trait]
impl<T> OperationalSettingsService for Arc<T>
where
    T: OperationalSettingsService + ?Sized,
{
    async fn load_announcement_read_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<String>, sqlx::Error> {
        (**self)
            .load_announcement_read_ids(database_file, user_id)
            .await
    }

    async fn save_announcements_read(
        &self,
        database_file: PathBuf,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .save_announcements_read(database_file, user_id, ids)
            .await
    }

    async fn load_claim_status(&self, database_file: PathBuf) -> Result<bool, sqlx::Error> {
        (**self).load_claim_status(database_file).await
    }

    async fn claim_initial_admin_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        email: String,
        password_hash: String,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        (**self)
            .claim_initial_admin_user(database_file, user_id, email, password_hash)
            .await
    }

    async fn load_client_settings_global(
        &self,
        database_file: PathBuf,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        (**self)
            .load_client_settings_global(database_file, allow_unauthorized_only)
            .await
    }

    async fn load_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Value, sqlx::Error> {
        (**self)
            .load_client_settings_user(database_file, user_id)
            .await
    }

    async fn upsert_client_settings_global(
        &self,
        database_file: PathBuf,
        settings: Vec<(String, String, bool)>,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .upsert_client_settings_global(database_file, settings)
            .await
    }

    async fn upsert_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        settings: Vec<(String, String)>,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .upsert_client_settings_user(database_file, user_id, settings)
            .await
    }

    async fn delete_client_settings_global(
        &self,
        database_file: PathBuf,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .delete_client_settings_global(database_file, keys)
            .await
    }

    async fn delete_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .delete_client_settings_user(database_file, user_id, keys)
            .await
    }

    fn list_directory_entries(&self, path: PathBuf, directories_only: bool) -> Vec<Value> {
        (**self).list_directory_entries(path, directories_only)
    }

    fn list_font_families(&self, path: PathBuf) -> Vec<String> {
        (**self).list_font_families(path)
    }

    fn load_font_family_css(&self, path: PathBuf, family: String) -> Option<String> {
        (**self).load_font_family_css(path, family)
    }

    fn load_font_file(&self, path: PathBuf, family: String, file: String) -> Option<Vec<u8>> {
        (**self).load_font_file(path, family, file)
    }

    async fn delete_syncpoints_by_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .delete_syncpoints_by_user(database_file, user_id)
            .await
    }

    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
        key_ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .delete_syncpoints_by_user_and_key_ids(database_file, user_id, key_ids)
            .await
    }

    async fn load_history_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        (**self)
            .load_history_page(database_file, page, size, sorts)
            .await
    }

    async fn load_page_hash_matches_page(
        &self,
        database_file: PathBuf,
        page_hash: String,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        (**self)
            .load_page_hash_matches_page(database_file, page_hash, page, size, sorts)
            .await
    }

    async fn load_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        (**self)
            .load_page_hash_thumbnail(database_file, page_hash)
            .await
    }

    async fn load_unknown_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        (**self)
            .load_unknown_page_hash_thumbnail(database_file, page_hash, resize_to)
            .await
    }

    async fn load_page_hashes_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        actions: Vec<String>,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        (**self)
            .load_page_hashes_page(database_file, page, size, actions, sorts)
            .await
    }

    async fn load_page_hashes_unknown_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        (**self)
            .load_page_hashes_unknown_page(database_file, page, size, sorts)
            .await
    }

    async fn load_page_hash_delete_targets(
        &self,
        database_file: PathBuf,
        hash: String,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
        (**self)
            .load_page_hash_delete_targets(database_file, hash)
            .await
    }

    async fn upsert_page_hash(
        &self,
        database_file: PathBuf,
        hash: String,
        size: Option<i64>,
        action: String,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .upsert_page_hash(database_file, hash, size, action)
            .await
    }

    fn analyze_transient_book(&self, path: String) -> TransientBookAnalysis {
        (**self).analyze_transient_book(path)
    }

    async fn infer_transient_series_and_number(
        &self,
        database_file: PathBuf,
        transient_name: String,
    ) -> (Option<String>, Option<f64>) {
        (**self)
            .infer_transient_series_and_number(database_file, transient_name)
            .await
    }

    fn list_transient_book_entries(&self, root: PathBuf) -> Vec<Value> {
        (**self).list_transient_book_entries(root)
    }

    async fn validate_transient_scan_root(
        &self,
        database_file: PathBuf,
        path: String,
    ) -> Result<(), String> {
        (**self)
            .validate_transient_scan_root(database_file, path)
            .await
    }

    fn load_transient_book_file_metadata(&self, path: String) -> Option<TransientBookFileMetadata> {
        (**self).load_transient_book_file_metadata(path)
    }

    fn load_transient_book_media(&self, path: String) -> Option<Vec<u8>> {
        (**self).load_transient_book_media(path)
    }

    fn transient_book_content_type(&self, path: String, media_type: String) -> &'static str {
        (**self).transient_book_content_type(path, media_type)
    }

    fn transient_book_page_content(
        &self,
        path: String,
        media_type: String,
        pages: Vec<TransientBookPage>,
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        (**self).transient_book_page_content(path, media_type, pages, page_number)
    }
}
