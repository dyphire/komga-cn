use std::sync::Arc;

use komga_application::operational::ServerSettingsPort;
use komga_application::runtime_sse::RuntimeSseEventSink;
use komga_application::task_processing::{CleanupEmptySetsPolicy, ThumbnailRegenerationPolicy};
use komga_config::env_config::RuntimeConfig;
use komga_config::writer_ownership::{WriterDecision, WriterKind};
use komga_infrastructure::configure_mobi_cache_root;
use komga_infrastructure::{DatabaseHandle, ServerSettingsStore};
use komga_infrastructure::{TaskRuntimeContext, TaskRuntimeOwnershipOverrides};
use komga_infrastructure::{
    connect_task_pool, connect_task_write_pool, default_read_max_connections,
};

pub(crate) async fn task_runtime_context(
    config: &RuntimeConfig,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
) -> TaskRuntimeContext {
    configure_mobi_cache_root(
        config
            .config_dir
            .clone()
            .map(|path| path.join("mobi-cache")),
    );
    let main_db = DatabaseHandle::file_backed(config.database_file.clone())
        .await
        .expect("failed to open main database");
    let task_write_pool = connect_task_write_pool(main_db.database_file())
        .await
        .expect("failed to create private write pool");
    let task_read_pool = connect_task_pool(main_db.database_file(), default_read_max_connections())
        .await
        .expect("failed to create private read pool");
    let runtime_policies = load_task_runtime_policies(config).await;
    TaskRuntimeContext::new(
        main_db,
        config.tasks_db_file.clone(),
        config.lucene_data_directory.clone(),
        matches!(
            config.writer_decision(WriterKind::TasksDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        ),
        config.task_pool_size,
        task_write_pool,
        task_read_pool,
    )
    .with_cleanup_empty_sets_policy(runtime_policies.cleanup_empty_sets)
    .with_thumbnail_regeneration_policy(runtime_policies.thumbnail_regeneration)
    .with_runtime_events(runtime_events)
    .with_ownership_overrides(TaskRuntimeOwnershipOverrides {
        owns_main_database: Some(matches!(
            config.writer_decision(WriterKind::MainDatabase),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
        owns_filesystem_scan_output: Some(matches!(
            config.writer_decision(WriterKind::FilesystemScanOutput),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
        owns_sidecar_output: Some(matches!(
            config.writer_decision(WriterKind::SidecarOutput),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
        owns_search_index: Some(matches!(
            config.writer_decision(WriterKind::SearchIndex),
            WriterDecision::Allowed | WriterDecision::Isolated
        )),
    })
}

struct TaskRuntimePolicies {
    cleanup_empty_sets: CleanupEmptySetsPolicy,
    thumbnail_regeneration: ThumbnailRegenerationPolicy,
}

async fn load_task_runtime_policies(config: &RuntimeConfig) -> TaskRuntimePolicies {
    let settings = ServerSettingsStore::new(config.database_file.clone())
        .load_settings()
        .await
        .expect("failed to load server settings for task runtime policies");

    TaskRuntimePolicies {
        cleanup_empty_sets: CleanupEmptySetsPolicy {
            delete_empty_collections: settings.delete_empty_collections,
            delete_empty_read_lists: settings.delete_empty_read_lists,
        },
        thumbnail_regeneration: ThumbnailRegenerationPolicy {
            generated_thumbnail_max_edge: settings.thumbnail_size.max_edge(),
        },
    }
}
