use super::*;
use async_trait::async_trait;
use komga_infrastructure::filesystem::browser as infrastructure_browser;
use komga_infrastructure::filesystem::fonts as infrastructure_fonts;
use komga_infrastructure::filesystem::transient_books as infrastructure_transient_books;
use komga_interfaces::operational_runtime_access::SqlitePoolSnapshot;
use komga_interfaces::operational_settings_access::{
    ClaimInitialAdminUserResult, PageHashDeleteTarget, PageHashDeleteTargetPage, PageHashThumbnail,
    TransientBookAnalysis, TransientBookFileMetadata, TransientBookPage,
};
use serde_json::Value;

#[derive(Clone, Default)]
pub(super) struct RuntimeOperationalRuntimeService;

#[async_trait]
impl OperationalRuntimeService for RuntimeOperationalRuntimeService {
    async fn load_task_execution_values(
        &self,
        tasks_db_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_task_execution_values(tasks_db_file.as_path())
            .await
    }

    async fn load_libraries_count(&self, database_file: PathBuf) -> Result<f64, String> {
        infrastructure_operational_metrics::load_libraries_count(database_file.as_path()).await
    }

    async fn load_series_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_series_grouped_by_library(database_file.as_path())
            .await
    }

    async fn load_books_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_books_grouped_by_library(database_file.as_path())
            .await
    }

    async fn load_books_filesize_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_books_filesize_grouped_by_library(
            database_file.as_path(),
        )
        .await
    }

    async fn load_sidecars_grouped_by_library(
        &self,
        database_file: PathBuf,
    ) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_sidecars_grouped_by_library(
            database_file.as_path(),
        )
        .await
    }

    async fn load_collections_count(&self, database_file: PathBuf) -> Result<f64, String> {
        infrastructure_operational_metrics::load_collections_count(database_file.as_path()).await
    }

    async fn load_readlists_count(&self, database_file: PathBuf) -> Result<f64, String> {
        infrastructure_operational_metrics::load_readlists_count(database_file.as_path()).await
    }

    async fn load_task_failure_count(&self, database_file: PathBuf) -> Result<f64, String> {
        infrastructure_operational_metrics::load_task_failure_count(database_file.as_path()).await
    }

    async fn load_sqlite_pool_snapshots(
        &self,
        paths: Vec<PathBuf>,
    ) -> Result<Vec<SqlitePoolSnapshot>, String> {
        Ok(
            infrastructure_operational_metrics::load_sqlite_pool_snapshots(paths.as_slice())
                .into_iter()
                .map(|snapshot| SqlitePoolSnapshot {
                    path: snapshot.path,
                    max_connections: snapshot.max_connections,
                    min_connections: snapshot.min_connections,
                    total_connections: snapshot.total_connections,
                    idle_connections: snapshot.idle_connections,
                    in_use_connections: snapshot.in_use_connections,
                    is_closed: snapshot.is_closed,
                })
                .collect(),
        )
    }
}

pub(super) fn compose_operational_runtime_service() -> RuntimeOperationalRuntimeService {
    RuntimeOperationalRuntimeService
}

#[derive(Clone, Default)]
pub(super) struct RuntimeOperationalSettingsService;

#[async_trait]
impl OperationalSettingsService for RuntimeOperationalSettingsService {
    async fn load_announcement_read_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Vec<String>, sqlx::Error> {
        komga_infrastructure::announcements_access::load_announcement_read_ids(
            database_file.as_path(),
            &user_id,
        )
        .await
    }

    async fn save_announcements_read(
        &self,
        database_file: PathBuf,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        komga_infrastructure::announcements_access::save_announcements_read(
            database_file.as_path(),
            &user_id,
            ids.as_slice(),
        )
        .await
    }

    async fn load_claim_status(&self, database_file: PathBuf) -> Result<bool, sqlx::Error> {
        komga_infrastructure::claims_access::load_claim_status(database_file.as_path()).await
    }

    async fn claim_initial_admin_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        email: String,
        password_hash: String,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        komga_infrastructure::claims_access::claim_initial_admin_user(
            database_file.as_path(),
            &user_id,
            &email,
            &password_hash,
        )
        .await
        .map(|value| match value {
            komga_infrastructure::claims_access::ClaimInitialAdminUserResult::Created(user) => {
                ClaimInitialAdminUserResult::Created(Box::new(
                    komga_application::identity_access::AuthUser {
                        id: user.id,
                        email: user.email,
                        password: String::new(),
                        roles: vec!["ADMIN".to_string()],
                        shared_all_libraries: true,
                        shared_library_ids: Vec::new(),
                        labels_allow: Vec::new(),
                        labels_exclude: Vec::new(),
                        age_restriction: None,
                    },
                ))
            }
            komga_infrastructure::claims_access::ClaimInitialAdminUserResult::AlreadyClaimed => {
                ClaimInitialAdminUserResult::AlreadyClaimed
            }
        })
    }

    async fn load_client_settings_global(
        &self,
        database_file: PathBuf,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_operational_settings::load_client_settings_global(
            database_file.as_path(),
            allow_unauthorized_only,
        )
        .await
    }

    async fn load_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_operational_settings::load_client_settings_user(
            database_file.as_path(),
            &user_id,
        )
        .await
    }

    async fn upsert_client_settings_global(
        &self,
        database_file: PathBuf,
        settings: Vec<(String, String, bool)>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::upsert_client_settings_global(
            database_file.as_path(),
            settings.as_slice(),
        )
        .await
    }

    async fn upsert_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        settings: Vec<(String, String)>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::upsert_client_settings_user(
            database_file.as_path(),
            &user_id,
            settings.as_slice(),
        )
        .await
    }

    async fn delete_client_settings_global(
        &self,
        database_file: PathBuf,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_client_settings_global(
            database_file.as_path(),
            keys.as_slice(),
        )
        .await
    }

    async fn delete_client_settings_user(
        &self,
        database_file: PathBuf,
        user_id: String,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_client_settings_user(
            database_file.as_path(),
            &user_id,
            keys.as_slice(),
        )
        .await
    }

    fn list_directory_entries(&self, path: PathBuf, directories_only: bool) -> Vec<Value> {
        infrastructure_browser::list_directory_entries(path.as_path(), directories_only)
    }

    fn list_font_families(&self, path: PathBuf) -> Vec<String> {
        infrastructure_fonts::list_font_families(path.as_path())
    }

    fn load_font_family_css(&self, path: PathBuf, family: String) -> Option<String> {
        infrastructure_fonts::load_font_family_css(path.as_path(), &family)
    }

    fn load_font_file(&self, path: PathBuf, family: String, file: String) -> Option<Vec<u8>> {
        infrastructure_fonts::load_font_file(path.as_path(), &family, &file)
    }

    async fn delete_syncpoints_by_user(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_syncpoints_by_user(
            database_file.as_path(),
            &user_id,
        )
        .await
    }

    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        database_file: PathBuf,
        user_id: String,
        key_ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_syncpoints_by_user_and_key_ids(
            database_file.as_path(),
            &user_id,
            key_ids.as_slice(),
        )
        .await
    }

    async fn load_history_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_operational_settings::load_history_page(
            database_file.as_path(),
            page,
            size,
            &sorts,
        )
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
        infrastructure_page_hashes::load_page_hash_matches_page(
            database_file.as_path(),
            &page_hash,
            page,
            size,
            &sorts,
        )
        .await
    }

    async fn load_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        infrastructure_page_hashes::load_page_hash_thumbnail(database_file.as_path(), &page_hash)
            .await
            .map(|value| {
                value.map(|row| PageHashThumbnail {
                    media_type: row.media_type,
                    bytes: row.bytes,
                })
            })
    }

    async fn load_unknown_page_hash_thumbnail(
        &self,
        database_file: PathBuf,
        page_hash: String,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        infrastructure_page_hashes::load_unknown_page_hash_thumbnail(
            database_file.as_path(),
            &page_hash,
            resize_to,
        )
        .await
        .map(|value| {
            value.map(|row| PageHashThumbnail {
                media_type: row.media_type,
                bytes: row.bytes,
            })
        })
    }

    async fn load_page_hashes_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        actions: Vec<String>,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_page_hashes::load_page_hashes_page(
            database_file.as_path(),
            page,
            size,
            &actions,
            &sorts,
        )
        .await
    }

    async fn load_page_hashes_unknown_page(
        &self,
        database_file: PathBuf,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_page_hashes::load_page_hashes_unknown_page(
            database_file.as_path(),
            page,
            size,
            &sorts,
        )
        .await
    }

    async fn load_page_hash_delete_targets(
        &self,
        database_file: PathBuf,
        hash: String,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
        infrastructure_page_hashes::load_page_hash_delete_targets(database_file.as_path(), &hash)
            .await
            .map(|targets| {
                targets
                    .into_iter()
                    .map(|target| PageHashDeleteTarget {
                        book_id: target.book_id,
                        pages: target
                            .pages
                            .into_iter()
                            .map(|page| PageHashDeleteTargetPage {
                                file_hash: page.file_hash,
                                file_size: page.file_size,
                                file_name: page.file_name,
                                media_type: page.media_type,
                                page_number: page.page_number,
                            })
                            .collect(),
                    })
                    .collect()
            })
    }

    async fn upsert_page_hash(
        &self,
        database_file: PathBuf,
        hash: String,
        size: Option<i64>,
        action: String,
    ) -> Result<(), sqlx::Error> {
        infrastructure_page_hashes::upsert_page_hash(database_file.as_path(), &hash, size, &action)
            .await
    }

    fn analyze_transient_book(&self, path: String) -> TransientBookAnalysis {
        let value = infrastructure_transient_books::analyze_transient_book(&path);
        TransientBookAnalysis {
            status: value.status,
            media_type: value.media_type,
            page_count: value.page_count,
            pages: value
                .pages
                .into_iter()
                .map(|page| TransientBookPage {
                    number: page.number,
                    file_name: page.file_name,
                    media_type: page.media_type,
                    width: page.width,
                    height: page.height,
                    size_bytes: page.size_bytes,
                })
                .collect(),
            files: value.files,
            comment: value.comment,
            number: value.number,
            series_id: value.series_id,
        }
    }

    async fn infer_transient_series_and_number(
        &self,
        database_file: PathBuf,
        transient_name: String,
    ) -> (Option<String>, Option<f64>) {
        infrastructure_transient_books::infer_transient_series_and_number(
            database_file.as_path(),
            &transient_name,
        )
        .await
    }

    fn list_transient_book_entries(&self, root: PathBuf) -> Vec<Value> {
        infrastructure_transient_books::list_transient_book_entries(root.as_path())
    }

    async fn validate_transient_scan_root(
        &self,
        database_file: PathBuf,
        path: String,
    ) -> Result<(), String> {
        infrastructure_transient_books::validate_transient_scan_root(
            database_file.as_path(),
            std::path::Path::new(&path),
        )
        .await
    }

    fn load_transient_book_file_metadata(&self, path: String) -> Option<TransientBookFileMetadata> {
        infrastructure_transient_books::load_transient_book_file_metadata(&path).map(|value| {
            TransientBookFileMetadata {
                file_last_modified_unix_nanos: value.file_last_modified_unix_nanos,
                size_bytes: value.size_bytes,
            }
        })
    }

    fn load_transient_book_media(&self, path: String) -> Option<Vec<u8>> {
        infrastructure_transient_books::load_transient_book_media(&path)
    }

    fn transient_book_content_type(&self, path: String, media_type: String) -> &'static str {
        infrastructure_transient_books::transient_book_content_type(&path, &media_type)
    }

    fn transient_book_page_content(
        &self,
        path: String,
        media_type: String,
        pages: Vec<TransientBookPage>,
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        let pages = pages
            .into_iter()
            .map(|page| infrastructure_transient_books::TransientBookPage {
                number: page.number,
                file_name: page.file_name,
                media_type: page.media_type,
                width: page.width,
                height: page.height,
                size_bytes: page.size_bytes,
            })
            .collect::<Vec<_>>();
        infrastructure_transient_books::transient_book_page_content(
            &path,
            &media_type,
            pages.as_slice(),
            page_number,
        )
    }
}

pub(super) fn compose_operational_settings_service() -> RuntimeOperationalSettingsService {
    RuntimeOperationalSettingsService
}
