use axum::Router;
use std::path::Path;
use tokio::net::TcpListener;

use crate::composition::start_server;
use crate::config::RuntimeConfig;
use komga_application::task_processing::{TaskRuntimeConfig, TaskRuntimeContext};
use komga_interfaces::http::state::RuntimeProfile;

pub fn build_router() -> Router {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    build_router_with_config(&config)
}

pub fn build_router_with_profile(profile: RuntimeProfile) -> Router {
    let config = RuntimeConfig::for_runtime_profile(match profile {
        RuntimeProfile::SnapshotAligned => crate::config::RuntimeProfile::SnapshotAligned,
        RuntimeProfile::LiveLocaldb => crate::config::RuntimeProfile::LiveLocaldb,
    });
    build_router_with_config(&config)
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    if matches!(
        config.runtime_profile,
        crate::config::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(config);
    }
    start_server::build_router_with_config(config)
}

pub async fn serve(listener: TcpListener) -> std::io::Result<()> {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    serve_with_config(listener, config).await
}

pub async fn serve_with_config(
    listener: TcpListener,
    config: RuntimeConfig,
) -> std::io::Result<()> {
    if matches!(
        config.runtime_profile,
        crate::config::RuntimeProfile::LiveLocaldb
    ) {
        crate::runtime::startup_scan::bootstrap_library_scan(&config);
    }
    start_server::serve_with_config(listener, config).await
}

pub fn invalidate_sessions_for_user(user_id: &str) {
    komga_interfaces::http::identity_access::auth::invalidate_user_sessions(user_id)
}

pub fn configure_remember_me_store_root(store_root: &Path) -> String {
    komga_interfaces::http::identity_access::auth::configure_remember_me_store(store_root)
}

impl TaskRuntimeConfig for RuntimeConfig {
    fn task_runtime_context(&self) -> TaskRuntimeContext {
        TaskRuntimeContext {
            database_file: self.database_file.clone(),
            tasks_db_file: self.tasks_db_file.clone(),
            lucene_data_directory: self.lucene_data_directory.clone(),
            consumes_queue: matches!(
                self.writer_decision(crate::config::WriterKind::TasksDatabase),
                crate::config::WriterDecision::Allowed | crate::config::WriterDecision::Isolated
            ),
        }
    }
}
