use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::http::{HeaderMap, StatusCode};
use komga_application::operational::{
    PersistedServerSettings, ServerSettingChange, ServerSettingsPort,
};

use super::{kobo_ping_for_tests, kobo_request_base_url, load_kobo_proxy_enabled};
use crate::access_log::RequestConnectionInfo;
use crate::state::RuntimeState;

#[tokio::test]
async fn kobo_ping_rejects_requests_without_valid_auth() {
    let identity = crate::state::tests::test_identity_state().await;
    let response = kobo_ping_for_tests(
        &identity,
        "invalid-token",
        RequestConnectionInfo::default(),
        HeaderMap::new(),
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn kobo_proxy_enabled_propagates_settings_load_errors() {
    let error = load_kobo_proxy_enabled(&FailingServerSettings)
        .await
        .expect_err("settings load failure should be propagated");

    assert_eq!(error.to_string(), "settings failed");
}

#[tokio::test]
async fn kobo_request_base_url_propagates_settings_load_errors() {
    let error = kobo_request_base_url(&FailingServerSettings, &runtime_state(), &HeaderMap::new())
        .await
        .expect_err("settings load failure should be propagated");

    assert_eq!(error.to_string(), "settings failed");
}

struct FailingServerSettings;

#[async_trait::async_trait]
impl ServerSettingsPort for FailingServerSettings {
    async fn load_map(&self) -> anyhow::Result<BTreeMap<String, Option<String>>> {
        Err(anyhow::anyhow!("settings failed"))
    }

    async fn load_settings(&self) -> anyhow::Result<PersistedServerSettings> {
        Err(anyhow::anyhow!("settings failed"))
    }

    async fn apply_changes(&self, _changes: &[ServerSettingChange]) -> anyhow::Result<()> {
        Ok(())
    }
}

fn runtime_state() -> RuntimeState {
    RuntimeState {
        tasks_db_file: PathBuf::from("tasks.db"),
        lucene_data_directory: PathBuf::from("lucene"),
        fonts_data_directory: PathBuf::from("fonts"),
        log_file: PathBuf::from("komga.log"),
        config_dir: None,
        bind_address: SocketAddr::from(([127, 0, 0, 1], 8080)),
        configuration_bind_address: SocketAddr::from(([127, 0, 0, 1], 8080)),
        server_context_path: None,
        configuration_server_context_path: None,
        actuator_enabled: false,
        dev_cors_enabled: false,
    }
}
