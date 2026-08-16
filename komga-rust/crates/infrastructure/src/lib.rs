use std::env;
use std::path::PathBuf;
use std::sync::OnceLock;

use pdfium_render::prelude::*;

mod announcements_access;
mod archive_builder;
mod auth;
mod claims_access;
mod content_resolver;
mod context;
mod database_handle;
mod discovery_detail_access;
mod discovery_persisted_access;
mod event_emitter_adapter;
mod filesystem;
mod library_catalog;
mod media_reader;
mod metadata;
mod mobi_cache;
mod opds_catalog_access;
mod opds_persisted_access;
mod operational_access;
mod operational_actuator_access;
mod operational_metrics_access;
mod parsing;
mod persisted_paths;
mod progress_writer;
mod random_tokens;
mod rar_support;
mod read_models;
mod runtime_identity_access;
mod search;
mod search_sync_adapter;
mod sql;
mod sqlite;
mod task_enqueue_adapter;
mod task_queue;
#[cfg(test)]
pub(crate) mod test_support;
mod thumbnail_writer;

pub use archive_builder::ZipArchiveBuilder;
pub use content_resolver::ContentResolver;
pub use context::{SqlitePersistenceConnection, SqlitePersistenceContext, SqliteUnitOfWork};
pub use database_handle::DatabaseHandle;
pub use discovery_detail_access::DiscoveryDetailAccess;
pub use discovery_persisted_access::{DiscoveryQuerySupportAccess, SqliteDiscoveryBrowseService};
pub use event_emitter_adapter::SseBookEventEmitter;
pub use filesystem::{FilesystemBookImport, remove_file_after_release};
pub use library_catalog::LibraryCatalogAccess;
pub use media_reader::MediaReader;
pub use metadata::{SqliteBookMetadataPort, generate_book_thumbnail};
pub use mobi_cache::configure_mobi_cache_root;
pub use opds_catalog_access::OpdsCatalogAccess;
pub use opds_persisted_access::OpdsPersistedAccess;
pub use operational_access::{
    AnnouncementAccess, ClaimAccess, ClientSettingsAccess, FilesystemBrowseAccess, FontAccess,
    HistoryAccess, PageHashAccess, RememberMeRuntimeSettings, RemoteFeedAccess, SyncpointAccess,
    TransientBookAccess, load_remember_me_runtime_settings,
};
pub use operational_actuator_access::ActuatorSnapshotAccess;
pub use operational_metrics_access::OperationalMetricsAccess;
pub use progress_writer::ProgressWriter;
pub use runtime_identity_access::{
    IdentityAccess, invalidate_user_sessions, persisted_update_password_by_user_id,
};
pub use search::{
    SearchEntityType, SearchIndexLifecycle, SearchStartupLifecycle, decide_startup_lifecycle,
    prepare_for_rebuild, rebuild_index_from_database, search_analyzer_version,
};
pub use search_sync_adapter::SearchSyncAdapter;
pub use sqlite::{
    DEFAULT_MAX_CONNECTIONS, InitialBootstrapUserWriteModel, PersistedBootstrapUser,
    ServerSettingsStore, SharedSqlitePoolSnapshot, SqliteTempPool, WRITE_MAX_CONNECTIONS,
    bootstrap_pool, bootstrap_tasks_pool, close_all_shared_pools, connect_main_write_context,
    connect_read_pool, connect_shared_pool, connect_task_pool, connect_task_write_pool,
    connect_write_pool, default_read_max_connections, evict_shared_pools_for_paths,
    file_backed_connect_options, list_persisted_user_emails, load_persisted_user_by_email,
    load_persisted_user_count, persist_initial_bootstrap_users, reject_or_quarantine_pool_topology,
    shared_pool_snapshots_for_paths, update_persisted_user_passwords,
};
pub use task_enqueue_adapter::TaskEnqueueAdapter;
pub use task_queue::{
    DatabaseRuntime, FilesystemRuntime, JobRuntime, RuntimeBackgroundState, SearchRuntime,
    SharedTaskQueue, SqliteFilesystemLibraryScanPipeline, TaskQueueScheduler, TaskQueueWakeSignal,
    TaskRuntimeConfig, TaskRuntimeContext, TaskRuntimeOwnershipOverrides, WorkerRuntime,
    cleanup_authentication_activity_once, prepare_task_queue, process_startup_library_scans,
    run_background_task_iteration, run_periodic_library_scan_iteration,
};
pub use thumbnail_writer::ThumbnailWriter;

pub(crate) use persisted_paths::{
    resolve_library_item_path, resolve_optional_library_item_path, resolve_rooted_path,
    resolve_stored_path,
};
pub(crate) use random_tokens::random_hex_token;

static PDFIUM: OnceLock<anyhow::Result<Pdfium>> = OnceLock::new();
const DEFAULT_PDFIUM_LIBRARY_PATH: &str = env!("KOMGA_PDFIUM_LIB_PATH");

pub(crate) fn load_pdfium() -> anyhow::Result<&'static Pdfium> {
    match PDFIUM.get_or_init(init_pdfium) {
        Ok(pdfium) => Ok(pdfium),
        Err(error) => Err(anyhow::anyhow!(error.to_string())),
    }
}

fn init_pdfium() -> anyhow::Result<Pdfium> {
    let mut attempted_paths = Vec::new();

    for library_path in pdfium_library_candidates(
        env::var_os("KOMGA_PDFIUM_LIB_PATH").map(PathBuf::from),
        env::current_exe().ok(),
    ) {
        attempted_paths.push(library_path.display().to_string());
        match Pdfium::bind_to_library(&library_path) {
            Ok(bindings) => return Ok(Pdfium::new(bindings)),
            Err(_) => continue,
        }
    }

    Pdfium::bind_to_system_library()
        .map(Pdfium::new)
        .map_err(|error| {
            let message = if attempted_paths.is_empty() {
                format!("failed to bind Pdfium from system libraries: {error}")
            } else {
                format!(
                    "failed to bind Pdfium from bundled candidates [{}] and system libraries: {error}",
                    attempted_paths.join(", ")
                )
            };
            anyhow::anyhow!(message)
        })
}

fn pdfium_library_candidates(
    runtime_override: Option<PathBuf>,
    executable: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    // Prefer colocated libraries so packaged releases and container images stay portable
    // after leaving the build machine. Fall back to the build-time vendor path for local
    // development, then finally to the system loader.
    if let Some(runtime_override) = runtime_override {
        candidates.push(runtime_override);
    }

    if let Some(bundled_path) = executable.and_then(bundled_pdfium_library_path) {
        candidates.push(bundled_path);
    }

    candidates.push(PathBuf::from(DEFAULT_PDFIUM_LIBRARY_PATH));
    candidates
}

fn bundled_pdfium_library_path(executable: PathBuf) -> Option<PathBuf> {
    Some(executable.parent()?.join(pdfium_library_file_name()))
}

fn pdfium_library_file_name() -> &'static str {
    match env::consts::OS {
        "linux" => "libpdfium.so",
        "macos" => "libpdfium.dylib",
        "windows" => "pdfium.dll",
        other => panic!("unsupported target os for Pdfium library file name: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_PDFIUM_LIBRARY_PATH, pdfium_library_candidates, pdfium_library_file_name};
    use std::path::{Path, PathBuf};

    #[test]
    fn pdfium_candidates_prefer_override_then_bundled_then_build_vendor() {
        let candidates = pdfium_library_candidates(
            Some(PathBuf::from("/runtime/pdfium/custom.so")),
            Some(PathBuf::from("/opt/komga/komga-riir")),
        );

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("/runtime/pdfium/custom.so"),
                Path::new("/opt/komga").join(pdfium_library_file_name()),
                PathBuf::from(DEFAULT_PDFIUM_LIBRARY_PATH),
            ]
        );
    }
}
