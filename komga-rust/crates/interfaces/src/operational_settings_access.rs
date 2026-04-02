#![allow(clippy::type_complexity, clippy::large_enum_variant)]

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::operational_runtime_access::ServerSettingsStore;
use komga_application::identity_access::AuthUser;
use serde_json::Value;

pub type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send + 'static>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedServerSettings {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_key: String,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: &'static str,
    pub task_pool_size: u64,
    pub server_port: Option<u16>,
    pub server_context_path: Option<String>,
    pub kobo_proxy: bool,
    pub kobo_port: Option<u16>,
}

#[derive(Clone)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_epoch_seconds: i64,
    pub size_bytes: u64,
}

#[derive(Clone)]
pub struct PageHashThumbnail {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

pub enum ClaimInitialAdminUserResult {
    Created(AuthUser),
    AlreadyClaimed,
}

#[derive(Clone)]
pub struct OperationalSettingsAccessBackend {
    pub load_announcement_read_ids:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<Vec<String>, sqlx::Error>> + Send + Sync>,
    pub save_announcements_read: Arc<
        dyn Fn(PathBuf, String, Vec<String>) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync,
    >,
    pub load_claim_status:
        Arc<dyn Fn(PathBuf) -> BoxFuture<Result<bool, sqlx::Error>> + Send + Sync>,
    pub claim_initial_admin_user: Arc<
        dyn Fn(
                PathBuf,
                String,
                String,
                String,
            ) -> BoxFuture<Result<ClaimInitialAdminUserResult, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub load_client_settings_global:
        Arc<dyn Fn(PathBuf, bool) -> BoxFuture<Result<Value, sqlx::Error>> + Send + Sync>,
    pub load_client_settings_user:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<Value, sqlx::Error>> + Send + Sync>,
    pub upsert_client_settings_global: Arc<
        dyn Fn(PathBuf, Vec<(String, String, bool)>) -> BoxFuture<Result<(), sqlx::Error>>
            + Send
            + Sync,
    >,
    pub upsert_client_settings_user: Arc<
        dyn Fn(PathBuf, String, Vec<(String, String)>) -> BoxFuture<Result<(), sqlx::Error>>
            + Send
            + Sync,
    >,
    pub delete_client_settings_global:
        Arc<dyn Fn(PathBuf, Vec<String>) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync>,
    pub delete_client_settings_user: Arc<
        dyn Fn(PathBuf, String, Vec<String>) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync,
    >,
    pub list_directory_entries: Arc<dyn Fn(PathBuf, bool) -> Vec<Value> + Send + Sync>,
    pub list_font_families: Arc<dyn Fn(PathBuf) -> Vec<String> + Send + Sync>,
    pub load_font_family_css: Arc<dyn Fn(PathBuf, String) -> Option<String> + Send + Sync>,
    pub load_font_file: Arc<dyn Fn(PathBuf, String, String) -> Option<Vec<u8>> + Send + Sync>,
    pub delete_syncpoints_by_user:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync>,
    pub delete_syncpoints_by_user_and_key_ids: Arc<
        dyn Fn(PathBuf, String, Vec<String>) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync,
    >,
    pub load_history_page:
        Arc<dyn Fn(PathBuf, u64, u64) -> BoxFuture<Result<Value, sqlx::Error>> + Send + Sync>,
    pub load_page_hash_matches_page: Arc<
        dyn Fn(PathBuf, String, u64, u64) -> BoxFuture<Result<Value, sqlx::Error>> + Send + Sync,
    >,
    pub load_page_hash_thumbnail: Arc<
        dyn Fn(PathBuf, String) -> BoxFuture<Result<Option<PageHashThumbnail>, sqlx::Error>>
            + Send
            + Sync,
    >,
    pub load_page_hashes_page:
        Arc<dyn Fn(PathBuf, u64, u64) -> BoxFuture<Result<Value, sqlx::Error>> + Send + Sync>,
    pub load_page_hashes_unknown_page:
        Arc<dyn Fn(PathBuf, u64, u64) -> BoxFuture<Result<Value, sqlx::Error>> + Send + Sync>,
    pub upsert_page_hash: Arc<
        dyn Fn(PathBuf, String, Option<i64>, String) -> BoxFuture<Result<(), sqlx::Error>>
            + Send
            + Sync,
    >,
    pub delete_all_page_hash_matches:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync>,
    pub delete_page_hash_match: Arc<
        dyn Fn(PathBuf, String, String, u64) -> BoxFuture<Result<(), sqlx::Error>> + Send + Sync,
    >,
    pub load_server_settings: Arc<
        dyn Fn(Arc<ServerSettingsStore>) -> BoxFuture<Result<PersistedServerSettings, String>>
            + Send
            + Sync,
    >,
    pub apply_server_settings_changes: Arc<
        dyn Fn(
                Arc<ServerSettingsStore>,
                Vec<(String, Option<String>)>,
            ) -> BoxFuture<Result<(), String>>
            + Send
            + Sync,
    >,
    pub analyze_transient_book:
        Arc<dyn Fn(String) -> Result<TransientBookAnalysis, String> + Send + Sync>,
    pub infer_transient_series_and_number:
        Arc<dyn Fn(PathBuf, String) -> BoxFuture<(Option<String>, Option<f64>)> + Send + Sync>,
    pub list_transient_book_entries: Arc<dyn Fn(PathBuf) -> Vec<Value> + Send + Sync>,
    pub load_transient_book_file_metadata:
        Arc<dyn Fn(String) -> Option<TransientBookFileMetadata> + Send + Sync>,
    pub load_transient_book_media: Arc<dyn Fn(String) -> Option<Vec<u8>> + Send + Sync>,
    pub transient_book_content_type: Arc<dyn Fn(String, String) -> &'static str + Send + Sync>,
    pub transient_book_exists: Arc<dyn Fn(String) -> bool + Send + Sync>,
    pub transient_book_media_type: Arc<dyn Fn(String) -> String + Send + Sync>,
    pub transient_book_page_content: Arc<
        dyn Fn(String, String, Vec<TransientBookPage>, u32) -> Option<(String, Vec<u8>)>
            + Send
            + Sync,
    >,
}

static BACKEND: OnceLock<OperationalSettingsAccessBackend> = OnceLock::new();
#[cfg(test)]
static TEST_BACKEND: OnceLock<OperationalSettingsAccessBackend> = OnceLock::new();

pub fn install_operational_settings_access(backend: OperationalSettingsAccessBackend) {
    let _ = BACKEND.set(backend);
}

fn backend() -> &'static OperationalSettingsAccessBackend {
    if let Some(backend) = BACKEND.get() {
        return backend;
    }

    #[cfg(test)]
    {
        TEST_BACKEND.get_or_init(default_test_backend)
    }

    #[cfg(not(test))]
    {
        panic!("operational settings access backend should be installed before use");
    }
}

#[cfg(test)]
fn default_test_backend() -> OperationalSettingsAccessBackend {
    OperationalSettingsAccessBackend {
        load_announcement_read_ids: Arc::new(|_, _| Box::pin(async { Ok(vec![]) })),
        save_announcements_read: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        load_claim_status: Arc::new(|_| Box::pin(async { Ok(false) })),
        claim_initial_admin_user: Arc::new(|_, _, _, _| {
            Box::pin(async {
                Ok(ClaimInitialAdminUserResult::Created(AuthUser {
                    id: "test-admin".to_string(),
                    email: "admin@example.org".to_string(),
                    password: String::new(),
                    roles: vec!["ADMIN".to_string()],
                    shared_all_libraries: true,
                    shared_library_ids: Vec::new(),
                    labels_allow: Vec::new(),
                    labels_exclude: Vec::new(),
                    age_restriction: None,
                }))
            })
        }),
        load_client_settings_global: Arc::new(|_, _| {
            Box::pin(async { Ok(Value::Object(Default::default())) })
        }),
        load_client_settings_user: Arc::new(|_, _| {
            Box::pin(async { Ok(Value::Object(Default::default())) })
        }),
        upsert_client_settings_global: Arc::new(|_, _| Box::pin(async { Ok(()) })),
        upsert_client_settings_user: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        delete_client_settings_global: Arc::new(|_, _| Box::pin(async { Ok(()) })),
        delete_client_settings_user: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        list_directory_entries: Arc::new(|_, _| Vec::new()),
        list_font_families: Arc::new(|_| Vec::new()),
        load_font_family_css: Arc::new(|_, _| None),
        load_font_file: Arc::new(|_, _, _| None),
        delete_syncpoints_by_user: Arc::new(|_, _| Box::pin(async { Ok(()) })),
        delete_syncpoints_by_user_and_key_ids: Arc::new(|_, _, _| Box::pin(async { Ok(()) })),
        load_history_page: Arc::new(|_, _, _| {
            Box::pin(async { Ok(Value::Object(Default::default())) })
        }),
        load_page_hash_matches_page: Arc::new(|_, _, _, _| {
            Box::pin(async { Ok(Value::Object(Default::default())) })
        }),
        load_page_hash_thumbnail: Arc::new(|_, _| Box::pin(async { Ok(None) })),
        load_page_hashes_page: Arc::new(|_, _, _| {
            Box::pin(async { Ok(Value::Object(Default::default())) })
        }),
        load_page_hashes_unknown_page: Arc::new(|_, _, _| {
            Box::pin(async { Ok(Value::Object(Default::default())) })
        }),
        upsert_page_hash: Arc::new(|_, _, _, _| Box::pin(async { Ok(()) })),
        delete_all_page_hash_matches: Arc::new(|_, _| Box::pin(async { Ok(()) })),
        delete_page_hash_match: Arc::new(|_, _, _, _| Box::pin(async { Ok(()) })),
        load_server_settings: Arc::new(|settings_store| {
            Box::pin(async move {
                let persisted = settings_store.load_map().await?;
                let remember_me_key = persisted
                    .get("REMEMBER_ME_KEY")
                    .and_then(|value| value.as_ref())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(server_settings::generate_remember_me_key);

                if !persisted.contains_key("REMEMBER_ME_KEY")
                    || persisted
                        .get("REMEMBER_ME_KEY")
                        .is_some_and(|value| value.as_deref().unwrap_or_default().trim().is_empty())
                {
                    settings_store
                        .apply_changes(&[(
                            "REMEMBER_ME_KEY".to_string(),
                            Some(remember_me_key.clone()),
                        )])
                        .await?;
                }

                Ok(PersistedServerSettings {
                    delete_empty_collections: persisted
                        .get("DELETE_EMPTY_COLLECTIONS")
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")),
                    delete_empty_read_lists: persisted
                        .get("DELETE_EMPTY_READLISTS")
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")),
                    remember_me_key,
                    remember_me_duration_days: persisted
                        .get("REMEMBER_ME_DURATION")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .unwrap_or(365),
                    thumbnail_size: match persisted
                        .get("THUMBNAIL_SIZE")
                        .and_then(|value| value.as_deref())
                    {
                        Some("MEDIUM") => "MEDIUM",
                        Some("LARGE") => "LARGE",
                        Some("XLARGE") => "XLARGE",
                        _ => "DEFAULT",
                    },
                    task_pool_size: persisted
                        .get("TASK_POOL_SIZE")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .unwrap_or(1),
                    server_port: persisted
                        .get("SERVER_PORT")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u16>().ok()),
                    server_context_path: persisted
                        .get("SERVER_CONTEXT_PATH")
                        .and_then(|value| value.as_ref())
                        .cloned(),
                    kobo_proxy: persisted
                        .get("KOBO_PROXY")
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")),
                    kobo_port: persisted
                        .get("KOBO_PORT")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u16>().ok()),
                })
            })
        }),
        apply_server_settings_changes: Arc::new(|settings_store, changes| {
            Box::pin(async move { settings_store.apply_changes(changes.as_slice()).await })
        }),
        analyze_transient_book: Arc::new(|_| {
            Err("transient book analysis is not configured for tests".to_string())
        }),
        infer_transient_series_and_number: Arc::new(|_, _| Box::pin(async { (None, None) })),
        list_transient_book_entries: Arc::new(|_| Vec::new()),
        load_transient_book_file_metadata: Arc::new(|_| None),
        load_transient_book_media: Arc::new(|_| None),
        transient_book_content_type: Arc::new(|_, _| "application/octet-stream"),
        transient_book_exists: Arc::new(|_| false),
        transient_book_media_type: Arc::new(|_| String::new()),
        transient_book_page_content: Arc::new(|_, _, _, _| None),
    }
}

pub(crate) mod announcements {
    use super::*;

    pub(crate) async fn load_announcement_read_ids(
        database_file: &std::path::Path,
        user_id: &str,
    ) -> Result<Vec<String>, sqlx::Error> {
        (backend().load_announcement_read_ids)(database_file.to_path_buf(), user_id.to_string())
            .await
    }

    pub(crate) async fn save_announcements_read(
        database_file: &std::path::Path,
        user_id: &str,
        ids: &[String],
    ) -> Result<(), sqlx::Error> {
        (backend().save_announcements_read)(
            database_file.to_path_buf(),
            user_id.to_string(),
            ids.to_vec(),
        )
        .await
    }
}

pub(crate) mod claims {
    pub(crate) use super::ClaimInitialAdminUserResult;
    use super::*;

    pub(crate) async fn load_claim_status(
        database_file: &std::path::Path,
    ) -> Result<bool, sqlx::Error> {
        (backend().load_claim_status)(database_file.to_path_buf()).await
    }

    pub(crate) async fn claim_initial_admin_user(
        database_file: &std::path::Path,
        user_id: &str,
        email: &str,
        password_hash: &str,
    ) -> Result<ClaimInitialAdminUserResult, sqlx::Error> {
        (backend().claim_initial_admin_user)(
            database_file.to_path_buf(),
            user_id.to_string(),
            email.to_string(),
            password_hash.to_string(),
        )
        .await
    }
}

pub(crate) mod client_settings {
    use super::*;

    pub(crate) async fn load_client_settings_global(
        database_file: &std::path::Path,
        allow_unauthorized_only: bool,
    ) -> Result<Value, sqlx::Error> {
        (backend().load_client_settings_global)(
            database_file.to_path_buf(),
            allow_unauthorized_only,
        )
        .await
    }

    pub(crate) async fn load_client_settings_user(
        database_file: &std::path::Path,
        user_id: &str,
    ) -> Result<Value, sqlx::Error> {
        (backend().load_client_settings_user)(database_file.to_path_buf(), user_id.to_string())
            .await
    }

    pub(crate) async fn persist_upsert_client_settings_global(
        database_file: &std::path::Path,
        settings: &[(String, String, bool)],
    ) -> Result<(), sqlx::Error> {
        (backend().upsert_client_settings_global)(database_file.to_path_buf(), settings.to_vec())
            .await
    }

    pub(crate) async fn persist_upsert_client_settings_user(
        database_file: &std::path::Path,
        user_id: &str,
        settings: &[(String, String)],
    ) -> Result<(), sqlx::Error> {
        (backend().upsert_client_settings_user)(
            database_file.to_path_buf(),
            user_id.to_string(),
            settings.to_vec(),
        )
        .await
    }

    pub(crate) async fn persist_delete_client_settings_global(
        database_file: &std::path::Path,
        keys: &[String],
    ) -> Result<(), sqlx::Error> {
        (backend().delete_client_settings_global)(database_file.to_path_buf(), keys.to_vec()).await
    }

    pub(crate) async fn persist_delete_client_settings_user(
        database_file: &std::path::Path,
        user_id: &str,
        keys: &[String],
    ) -> Result<(), sqlx::Error> {
        (backend().delete_client_settings_user)(
            database_file.to_path_buf(),
            user_id.to_string(),
            keys.to_vec(),
        )
        .await
    }
}

pub(crate) mod filesystem {
    use super::*;

    pub(crate) fn list_directory_entries(
        path: &std::path::Path,
        directories_only: bool,
    ) -> Vec<Value> {
        (backend().list_directory_entries)(path.to_path_buf(), directories_only)
    }
}

pub(crate) mod fonts {
    use super::*;

    pub(crate) fn list_font_families(path: &std::path::Path) -> Vec<String> {
        (backend().list_font_families)(path.to_path_buf())
    }

    pub(crate) fn load_font_family_css(path: &std::path::Path, family: &str) -> Option<String> {
        (backend().load_font_family_css)(path.to_path_buf(), family.to_string())
    }

    pub(crate) fn load_font_file(
        path: &std::path::Path,
        family: &str,
        file: &str,
    ) -> Option<Vec<u8>> {
        (backend().load_font_file)(path.to_path_buf(), family.to_string(), file.to_string())
    }
}

pub(crate) mod operations {
    use super::*;

    pub(crate) async fn delete_syncpoints_by_user(
        database_file: &std::path::Path,
        user_id: &str,
    ) -> Result<(), sqlx::Error> {
        (backend().delete_syncpoints_by_user)(database_file.to_path_buf(), user_id.to_string())
            .await
    }

    pub(crate) async fn delete_syncpoints_by_user_and_key_ids(
        database_file: &std::path::Path,
        user_id: &str,
        key_ids: &[String],
    ) -> Result<(), sqlx::Error> {
        (backend().delete_syncpoints_by_user_and_key_ids)(
            database_file.to_path_buf(),
            user_id.to_string(),
            key_ids.to_vec(),
        )
        .await
    }

    pub(crate) async fn load_history_page(
        database_file: &std::path::Path,
        page: u64,
        size: u64,
    ) -> Result<Value, sqlx::Error> {
        (backend().load_history_page)(database_file.to_path_buf(), page, size).await
    }
}

pub(crate) mod page_hashes {
    use super::*;

    pub(crate) async fn load_page_hash_matches_page(
        database_file: &std::path::Path,
        page_hash: &str,
        page: u64,
        size: u64,
    ) -> Result<Value, sqlx::Error> {
        (backend().load_page_hash_matches_page)(
            database_file.to_path_buf(),
            page_hash.to_string(),
            page,
            size,
        )
        .await
    }

    pub(crate) async fn load_page_hash_thumbnail(
        database_file: &std::path::Path,
        page_hash: &str,
    ) -> Result<Option<PageHashThumbnail>, sqlx::Error> {
        (backend().load_page_hash_thumbnail)(database_file.to_path_buf(), page_hash.to_string())
            .await
    }

    pub(crate) async fn load_page_hashes_page(
        database_file: &std::path::Path,
        page: u64,
        size: u64,
    ) -> Result<Value, sqlx::Error> {
        (backend().load_page_hashes_page)(database_file.to_path_buf(), page, size).await
    }

    pub(crate) async fn load_page_hashes_unknown_page(
        database_file: &std::path::Path,
        page: u64,
        size: u64,
    ) -> Result<Value, sqlx::Error> {
        (backend().load_page_hashes_unknown_page)(database_file.to_path_buf(), page, size).await
    }

    pub(crate) async fn upsert_page_hash(
        database_file: &std::path::Path,
        hash: &str,
        size: Option<i64>,
        action: &str,
    ) -> Result<(), sqlx::Error> {
        (backend().upsert_page_hash)(
            database_file.to_path_buf(),
            hash.to_string(),
            size,
            action.to_string(),
        )
        .await
    }

    pub(crate) async fn delete_all_page_hash_matches(
        database_file: &std::path::Path,
        hash: &str,
    ) -> Result<(), sqlx::Error> {
        (backend().delete_all_page_hash_matches)(database_file.to_path_buf(), hash.to_string())
            .await
    }

    pub(crate) async fn delete_page_hash_match(
        database_file: &std::path::Path,
        hash: &str,
        media_id: &str,
        page_number: u64,
    ) -> Result<(), sqlx::Error> {
        (backend().delete_page_hash_match)(
            database_file.to_path_buf(),
            hash.to_string(),
            media_id.to_string(),
            page_number,
        )
        .await
    }
}

pub(crate) mod server_settings {
    pub(crate) use super::PersistedServerSettings;
    use super::*;

    pub(crate) async fn load_server_settings(
        settings_store: &ServerSettingsStore,
    ) -> Result<PersistedServerSettings, String> {
        (backend().load_server_settings)(Arc::new(settings_store.clone())).await
    }

    pub(crate) async fn apply_server_settings_changes(
        settings_store: &ServerSettingsStore,
        changes: &[(String, Option<String>)],
    ) -> Result<(), String> {
        (backend().apply_server_settings_changes)(
            Arc::new(settings_store.clone()),
            changes.to_vec(),
        )
        .await
    }

    pub(crate) fn generate_remember_me_key() -> String {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let sequence = COUNTER.fetch_add(1, Ordering::Relaxed);
        let raw = format!("{nanos:032x}{sequence:016x}");
        raw.chars().take(32).collect()
    }
}

pub(crate) mod transient_books {
    pub(crate) type InfrastructureTransientBookPage = super::TransientBookPage;

    use super::*;

    pub(crate) fn analyze_transient_book(path: &str) -> Result<TransientBookAnalysis, String> {
        (backend().analyze_transient_book)(path.to_string())
    }

    pub(crate) async fn infer_transient_series_and_number(
        database_file: &std::path::Path,
        transient_name: &str,
    ) -> (Option<String>, Option<f64>) {
        (backend().infer_transient_series_and_number)(
            database_file.to_path_buf(),
            transient_name.to_string(),
        )
        .await
    }

    pub(crate) fn list_transient_book_entries(root: &std::path::Path) -> Vec<Value> {
        (backend().list_transient_book_entries)(root.to_path_buf())
    }

    pub(crate) fn load_transient_book_file_metadata(
        path: &str,
    ) -> Option<TransientBookFileMetadata> {
        (backend().load_transient_book_file_metadata)(path.to_string())
    }

    pub(crate) fn transient_book_exists(path: &str) -> bool {
        (backend().transient_book_exists)(path.to_string())
    }

    pub(crate) fn transient_book_media_type(path: &str) -> String {
        (backend().transient_book_media_type)(path.to_string())
    }

    pub(crate) fn transient_book_page_content(
        path: &str,
        media_type: &str,
        pages: &[InfrastructureTransientBookPage],
        page_number: u32,
    ) -> Option<(String, Vec<u8>)> {
        (backend().transient_book_page_content)(
            path.to_string(),
            media_type.to_string(),
            pages.to_vec(),
            page_number,
        )
    }
}
