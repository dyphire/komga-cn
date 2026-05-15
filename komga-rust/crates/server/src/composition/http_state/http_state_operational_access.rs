use super::*;
use async_trait::async_trait;
use komga_application::media_assets::{PageHashDeleteTarget, PageHashThumbnail};
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::filesystem::browser;
use komga_infrastructure::filesystem::fonts;
use komga_infrastructure::filesystem::transient_books;
use komga_interfaces::state::{
    ClaimInitialAdminUserResult, OperationalRuntimeService, OperationalSettingsService,
    SqlitePoolSnapshot, TransientBookAnalysis, TransientBookFileMetadata, TransientBookPage,
};
use serde_json::Value;

#[derive(Clone)]
pub(super) struct RuntimeOperationalRuntimeService {
    main_db: DatabaseHandle,
    tasks_db: DatabaseHandle,
}

#[async_trait]
impl OperationalRuntimeService for RuntimeOperationalRuntimeService {
    async fn load_task_execution_values(&self) -> Result<Vec<(String, f64)>, String> {
        operational_metrics_access::load_task_execution_values(self.tasks_db.read_pool()).await
    }

    async fn load_libraries_count(&self) -> Result<f64, String> {
        operational_metrics_access::load_libraries_count(self.main_db.read_pool()).await
    }

    async fn load_series_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        operational_metrics_access::load_series_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_books_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        operational_metrics_access::load_books_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_books_filesize_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        operational_metrics_access::load_books_filesize_grouped_by_library(self.main_db.read_pool())
            .await
    }

    async fn load_sidecars_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        operational_metrics_access::load_sidecars_grouped_by_library(self.main_db.read_pool()).await
    }

    async fn load_collections_count(&self) -> Result<f64, String> {
        operational_metrics_access::load_collections_count(self.main_db.read_pool()).await
    }

    async fn load_readlists_count(&self) -> Result<f64, String> {
        operational_metrics_access::load_readlists_count(self.main_db.read_pool()).await
    }

    async fn load_task_failure_count(&self) -> Result<f64, String> {
        operational_metrics_access::load_task_failure_count(self.main_db.read_pool()).await
    }

    async fn load_sqlite_pool_snapshots(
        &self,
        paths: &[PathBuf],
    ) -> Result<Vec<SqlitePoolSnapshot>, String> {
        Ok(
            operational_metrics_access::load_sqlite_pool_snapshots(paths)
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

pub(super) fn compose_operational_runtime_service(
    main_db: DatabaseHandle,
    tasks_db: DatabaseHandle,
) -> RuntimeOperationalRuntimeService {
    RuntimeOperationalRuntimeService { main_db, tasks_db }
}

#[derive(Clone)]
pub(super) struct RuntimeOperationalSettingsService {
    db: DatabaseHandle,
}

#[async_trait]
impl OperationalSettingsService for RuntimeOperationalSettingsService {
    async fn load_announcement_read_ids(&self, user_id: &str) -> Result<Vec<String>, sqlx::Error> {
        komga_infrastructure::announcements_access::load_announcement_read_ids(
            self.db.read_pool(),
            user_id,
        )
        .await
    }

    async fn save_announcements_read(
        &self,
        user_id: &str,
        ids: &[String],
    ) -> Result<(), sqlx::Error> {
        komga_infrastructure::announcements_access::save_announcements_read(
            self.db.write_pool(),
            user_id,
            ids,
        )
        .await
    }

    async fn load_claim_status(&self) -> Result<bool, sqlx::Error> {
        komga_infrastructure::claims_access::load_claim_status(self.db.read_pool()).await
    }

    async fn claim_initial_admin_user(
        &self,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        komga_infrastructure::claims_access::claim_initial_admin_user(
            self.db.write_pool(),
            user_id,
            email,
            password_hash,
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
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        operational_settings_access::load_client_settings_global(
            self.db.read_pool(),
            allow_unauthorized_only,
        )
        .await
    }

    async fn load_client_settings_user(&self, user_id: &str) -> Result<Value, sqlx::Error> {
        operational_settings_access::load_client_settings_user(self.db.read_pool(), user_id).await
    }

    async fn upsert_client_settings_global(
        &self,
        settings: &[(String, String, bool)],
    ) -> Result<(), sqlx::Error> {
        operational_settings_access::upsert_client_settings_global(self.db.write_pool(), settings)
            .await
    }

    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &[(String, String)],
    ) -> Result<(), sqlx::Error> {
        operational_settings_access::upsert_client_settings_user(
            self.db.write_pool(),
            user_id,
            settings,
        )
        .await
    }

    async fn delete_client_settings_global(&self, keys: &[String]) -> Result<(), sqlx::Error> {
        operational_settings_access::delete_client_settings_global(self.db.write_pool(), keys).await
    }

    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), sqlx::Error> {
        operational_settings_access::delete_client_settings_user(
            self.db.write_pool(),
            user_id,
            keys,
        )
        .await
    }

    fn list_directory_entries(&self, path: &Path, directories_only: bool) -> Vec<Value> {
        browser::list_directory_entries(path, directories_only)
    }

    fn list_font_families(&self, path: &Path) -> Vec<String> {
        fonts::list_font_families(path)
    }

    fn load_font_family_css(&self, path: &Path, family: &str) -> Option<String> {
        fonts::load_font_family_css(path, family)
    }

    fn load_font_file(&self, path: &Path, family: &str, file: &str) -> Option<Vec<u8>> {
        fonts::load_font_file(path, family, file)
    }

    async fn delete_syncpoints_by_user(&self, user_id: &str) -> Result<(), sqlx::Error> {
        operational_settings_access::delete_syncpoints_by_user(self.db.write_pool(), user_id).await
    }

    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> Result<(), sqlx::Error> {
        operational_settings_access::delete_syncpoints_by_user_and_key_ids(
            self.db.write_pool(),
            user_id,
            key_ids,
        )
        .await
    }

    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        operational_settings_access::load_history_page(self.db.read_pool(), page, size, &sorts)
            .await
    }

    async fn load_page_hash_matches_page(
        &self,
        page_hash: &str,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, sqlx::Error> {
        page_hashes_access::load_page_hash_matches_page(
            self.db.read_pool(),
            page_hash,
            page,
            size,
            sorts,
        )
        .await
    }

    async fn load_page_hash_thumbnail(
        &self,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        page_hashes_access::load_page_hash_thumbnail(self.db.read_pool(), page_hash).await
    }

    async fn load_unknown_page_hash_thumbnail(
        &self,
        page_hash: &str,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        page_hashes_access::load_unknown_page_hash_thumbnail(
            self.db.read_pool(),
            page_hash,
            resize_to,
        )
        .await
    }

    async fn load_page_hashes_page(
        &self,
        page: u64,
        size: u64,
        actions: &[String],
        sorts: &[String],
    ) -> Result<Value, sqlx::Error> {
        page_hashes_access::load_page_hashes_page(self.db.read_pool(), page, size, actions, sorts)
            .await
    }

    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: &[String],
    ) -> Result<Value, sqlx::Error> {
        page_hashes_access::load_page_hashes_unknown_page(self.db.read_pool(), page, size, sorts)
            .await
    }

    async fn load_page_hash_delete_targets(
        &self,
        hash: &str,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
        page_hashes_access::load_page_hash_delete_targets(self.db.read_pool(), hash).await
    }

    async fn upsert_page_hash(
        &self,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), sqlx::Error> {
        page_hashes_access::upsert_page_hash(
            self.db.read_pool(),
            self.db.write_pool(),
            hash,
            size,
            action,
        )
        .await
    }

    fn analyze_transient_book(&self, path: &str) -> TransientBookAnalysis {
        let value = transient_books::analyze_transient_book(path);
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
        transient_name: &str,
    ) -> (Option<String>, Option<f64>) {
        transient_books::infer_transient_series_and_number(self.db.read_pool(), transient_name)
            .await
    }

    fn list_transient_book_entries(&self, root: &Path) -> Vec<Value> {
        transient_books::list_transient_book_entries(root)
    }

    async fn validate_transient_scan_root(&self, path: &str) -> Result<(), String> {
        transient_books::validate_transient_scan_root(self.db.read_pool(), Path::new(path)).await
    }

    fn load_transient_book_file_metadata(&self, path: &str) -> Option<TransientBookFileMetadata> {
        transient_books::load_transient_book_file_metadata(path).map(|value| {
            TransientBookFileMetadata {
                file_last_modified_unix_nanos: value.file_last_modified_unix_nanos,
                size_bytes: value.size_bytes,
            }
        })
    }

    fn load_transient_book_media(&self, path: &str) -> Option<Vec<u8>> {
        transient_books::load_transient_book_media(path)
    }

    fn transient_book_content_type(&self, path: &str, media_type: &str) -> &'static str {
        transient_books::transient_book_content_type(path, media_type)
    }

    fn transient_book_page_content(
        &self,
        path: &str,
        media_type: &str,
        pages: &[TransientBookPage],
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        let pages = pages
            .iter()
            .map(|page| transient_books::TransientBookPage {
                number: page.number,
                file_name: page.file_name.clone(),
                media_type: page.media_type.clone(),
                width: page.width,
                height: page.height,
                size_bytes: page.size_bytes,
            })
            .collect::<Vec<_>>();
        transient_books::transient_book_page_content(
            path,
            media_type,
            pages.as_slice(),
            page_number,
        )
    }
}

pub(super) fn compose_operational_settings_service(
    db: DatabaseHandle,
) -> RuntimeOperationalSettingsService {
    RuntimeOperationalSettingsService { db }
}
