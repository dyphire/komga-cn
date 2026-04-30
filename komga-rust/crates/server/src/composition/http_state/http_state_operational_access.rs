use super::*;
use async_trait::async_trait;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::filesystem::browser as infrastructure_browser;
use komga_infrastructure::filesystem::fonts as infrastructure_fonts;
use komga_infrastructure::filesystem::transient_books as infrastructure_transient_books;
use komga_interfaces::state::{
    ClaimInitialAdminUserResult, OperationalRuntimeService, OperationalSettingsService,
    PageHashDeleteTarget, PageHashDeleteTargetPage, PageHashThumbnail, SqlitePoolSnapshot,
    TransientBookAnalysis, TransientBookFileMetadata, TransientBookPage,
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
        infrastructure_operational_metrics::load_task_execution_values(
            self.tasks_db.database_file(),
        )
        .await
    }

    async fn load_libraries_count(&self) -> Result<f64, String> {
        infrastructure_operational_metrics::load_libraries_count(self.main_db.database_file()).await
    }

    async fn load_series_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_series_grouped_by_library(
            self.main_db.database_file(),
        )
        .await
    }

    async fn load_books_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_books_grouped_by_library(
            self.main_db.database_file(),
        )
        .await
    }

    async fn load_books_filesize_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_books_filesize_grouped_by_library(
            self.main_db.database_file(),
        )
        .await
    }

    async fn load_sidecars_grouped_by_library(&self) -> Result<Vec<(String, f64)>, String> {
        infrastructure_operational_metrics::load_sidecars_grouped_by_library(
            self.main_db.database_file(),
        )
        .await
    }

    async fn load_collections_count(&self) -> Result<f64, String> {
        infrastructure_operational_metrics::load_collections_count(self.main_db.database_file())
            .await
    }

    async fn load_readlists_count(&self) -> Result<f64, String> {
        infrastructure_operational_metrics::load_readlists_count(self.main_db.database_file()).await
    }

    async fn load_task_failure_count(&self) -> Result<f64, String> {
        infrastructure_operational_metrics::load_task_failure_count(self.main_db.database_file())
            .await
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
    async fn load_announcement_read_ids(
        &self,
        user_id: String,
    ) -> Result<Vec<String>, sqlx::Error> {
        komga_infrastructure::announcements_access::load_announcement_read_ids(
            self.db.database_file(),
            &user_id,
        )
        .await
    }

    async fn save_announcements_read(
        &self,
        user_id: String,
        ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        komga_infrastructure::announcements_access::save_announcements_read(
            self.db.database_file(),
            &user_id,
            ids.as_slice(),
        )
        .await
    }

    async fn load_claim_status(&self) -> Result<bool, sqlx::Error> {
        komga_infrastructure::claims_access::load_claim_status(self.db.database_file()).await
    }

    async fn claim_initial_admin_user(
        &self,
        user_id: String,
        email: String,
        password_hash: String,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        komga_infrastructure::claims_access::claim_initial_admin_user(
            self.db.database_file(),
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
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_operational_settings::load_client_settings_global(
            self.db.database_file(),
            allow_unauthorized_only,
        )
        .await
    }

    async fn load_client_settings_user(&self, user_id: String) -> Result<Value, sqlx::Error> {
        infrastructure_operational_settings::load_client_settings_user(
            self.db.database_file(),
            &user_id,
        )
        .await
    }

    async fn upsert_client_settings_global(
        &self,
        settings: Vec<(String, String, bool)>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::upsert_client_settings_global(
            self.db.database_file(),
            settings.as_slice(),
        )
        .await
    }

    async fn upsert_client_settings_user(
        &self,
        user_id: String,
        settings: Vec<(String, String)>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::upsert_client_settings_user(
            self.db.database_file(),
            &user_id,
            settings.as_slice(),
        )
        .await
    }

    async fn delete_client_settings_global(&self, keys: Vec<String>) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_client_settings_global(
            self.db.database_file(),
            keys.as_slice(),
        )
        .await
    }

    async fn delete_client_settings_user(
        &self,
        user_id: String,
        keys: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_client_settings_user(
            self.db.database_file(),
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

    async fn delete_syncpoints_by_user(&self, user_id: String) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_syncpoints_by_user(
            self.db.database_file(),
            &user_id,
        )
        .await
    }

    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: String,
        key_ids: Vec<String>,
    ) -> Result<(), sqlx::Error> {
        infrastructure_operational_settings::delete_syncpoints_by_user_and_key_ids(
            self.db.database_file(),
            &user_id,
            key_ids.as_slice(),
        )
        .await
    }

    async fn load_history_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_operational_settings::load_history_page(
            self.db.database_file(),
            page,
            size,
            &sorts,
        )
        .await
    }

    async fn load_page_hash_matches_page(
        &self,
        page_hash: String,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_page_hashes::load_page_hash_matches_page(
            self.db.database_file(),
            &page_hash,
            page,
            size,
            &sorts,
        )
        .await
    }

    async fn load_page_hash_thumbnail(
        &self,
        page_hash: String,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        infrastructure_page_hashes::load_page_hash_thumbnail(self.db.database_file(), &page_hash)
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
        page_hash: String,
        resize_to: Option<u32>,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        infrastructure_page_hashes::load_unknown_page_hash_thumbnail(
            self.db.database_file(),
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
        page: u64,
        size: u64,
        actions: Vec<String>,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_page_hashes::load_page_hashes_page(
            self.db.database_file(),
            page,
            size,
            &actions,
            &sorts,
        )
        .await
    }

    async fn load_page_hashes_unknown_page(
        &self,
        page: u64,
        size: u64,
        sorts: Vec<String>,
    ) -> Result<Value, sqlx::Error> {
        infrastructure_page_hashes::load_page_hashes_unknown_page(
            self.db.database_file(),
            page,
            size,
            &sorts,
        )
        .await
    }

    async fn load_page_hash_delete_targets(
        &self,
        hash: String,
    ) -> Result<Vec<PageHashDeleteTarget>, sqlx::Error> {
        infrastructure_page_hashes::load_page_hash_delete_targets(self.db.database_file(), &hash)
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
        hash: String,
        size: Option<i64>,
        action: String,
    ) -> Result<(), sqlx::Error> {
        infrastructure_page_hashes::upsert_page_hash(self.db.database_file(), &hash, size, &action)
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
        transient_name: String,
    ) -> (Option<String>, Option<f64>) {
        infrastructure_transient_books::infer_transient_series_and_number(
            self.db.database_file(),
            &transient_name,
        )
        .await
    }

    fn list_transient_book_entries(&self, root: PathBuf) -> Vec<Value> {
        infrastructure_transient_books::list_transient_book_entries(root.as_path())
    }

    async fn validate_transient_scan_root(&self, path: String) -> Result<(), String> {
        infrastructure_transient_books::validate_transient_scan_root(
            self.db.database_file(),
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

pub(super) fn compose_operational_settings_service(
    db: DatabaseHandle,
) -> RuntimeOperationalSettingsService {
    RuntimeOperationalSettingsService { db }
}
