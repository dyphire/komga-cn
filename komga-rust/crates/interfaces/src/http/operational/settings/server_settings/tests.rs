use super::*;

use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::body::to_bytes;
use axum::http::{HeaderValue, header};
use komga_application::identity_access::AuthUser;
use komga_domain::discovery::DiscoveryQueryContext;

use crate::http::identity_access::auth::{
    configure_remember_me_store, session_token_for_user_with_namespace,
};
use crate::http::state::{
    BookImportSseEvent, LibraryCatalogOperations, OAuth2ClientConfig, RemoteCacheEntry,
    RuntimeState, SseOperationalState, TransientBooksStore,
};
use crate::operational_runtime_access::ServerSettingsStore;

#[tokio::test]
async fn update_server_settings_does_not_apply_runtime_task_pool_before_persistence_succeeds() {
    let fixture_root = unique_fixture_root("server-settings-persistence-failure");
    std::fs::create_dir_all(&fixture_root).expect("fixture root should be created");
    let database_file = fixture_root.join("main.db");
    let persisted_settings = Arc::new(Mutex::new(HashMap::from([
        (
            "REMEMBER_ME_KEY".to_string(),
            Some("seeded-remember-me-key".to_string()),
        ),
        ("TASK_POOL_SIZE".to_string(), Some("1".to_string())),
    ])));
    let persist_attempts = Arc::new(AtomicUsize::new(0));
    let settings_store = Arc::new(fake_settings_store(
        persisted_settings.clone(),
        persist_attempts.clone(),
    ));

    let apply_count = Arc::new(AtomicUsize::new(0));
    let state = test_operational_state(
        database_file.clone(),
        fixture_root.clone(),
        settings_store.clone(),
        {
            let apply_count = apply_count.clone();
            move |_value| {
                apply_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        },
    );
    let headers = admin_headers(&fixture_root);

    let response = update_server_settings(
        Extension(state.clone()),
        headers,
        Bytes::from(serde_json::json!({ "taskPoolSize": 4_u64 }).to_string()),
    )
    .await;

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let response_body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("settings error response body should be readable");
    let response_body: Value = serde_json::from_slice(&response_body)
        .expect("settings error response should be valid JSON");
    assert!(response_body.get("message").is_some());
    assert_eq!(apply_count.load(Ordering::SeqCst), 0);
    assert_eq!(persist_attempts.load(Ordering::SeqCst), 1);

    let persisted = settings_store
        .load_map()
        .await
        .expect("settings should remain readable after failure");
    assert_eq!(
        persisted.get("TASK_POOL_SIZE"),
        Some(&Some("1".to_string()))
    );

    std::fs::remove_dir_all(&fixture_root).expect("fixture root should be removed");
}

fn test_operational_state<F>(
    database_file: PathBuf,
    fixture_root: PathBuf,
    settings_store: Arc<ServerSettingsStore>,
    apply: F,
) -> OperationalState
where
    F: Fn(usize) -> Result<(), String> + Send + Sync + 'static,
{
    OperationalState {
        runtime: RuntimeState {
            database_file,
            tasks_db_file: fixture_root.join("tasks.db"),
            lucene_data_directory: fixture_root.join("lucene"),
            fonts_data_directory: fixture_root.join("fonts"),
            log_file: fixture_root.join("komga.log"),
            config_dir: Some(fixture_root.clone()),
            bind_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)),
            server_context_path: None,
            kepubify_path: None,
        },
        webui_assets_root: None,
        settings_store,
        oauth2_clients: Arc::new(Vec::<OAuth2ClientConfig>::new()),
        enqueue_task_records: Arc::new(|_, _| Ok(())),
        clear_unowned_tasks: Arc::new(|| 0),
        count_task_queue_by_type: Arc::new(BTreeMap::new),
        apply_task_pool_size: Arc::new(apply),
        library_catalog: test_library_catalog_operations(),
        sse: Arc::new(Mutex::new(SseOperationalState {
            accepting_connections: true,
            book_import_events: Vec::<BookImportSseEvent>::new(),
        })),
        announcements_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        releases_cache: Arc::new(Mutex::new(None::<RemoteCacheEntry>)),
        load_transient_books_records: Arc::new(|| Ok(std::collections::HashMap::new())),
        persist_transient_books_records: Arc::new(|_| Ok(())),
        transient_books: Arc::new(Mutex::new(TransientBooksStore::with_records(
            std::collections::HashMap::new(),
        ))),
        shutdown_trigger: None,
    }
}

fn fake_settings_store(
    persisted: Arc<Mutex<HashMap<String, Option<String>>>>,
    persist_attempts: Arc<AtomicUsize>,
) -> ServerSettingsStore {
    ServerSettingsStore::new(
        Arc::new({
            let persisted = persisted.clone();
            move || {
                let persisted = persisted.clone();
                Box::pin(async move {
                    Ok(persisted
                        .lock()
                        .expect("fake settings store should lock")
                        .clone())
                })
            }
        }),
        Arc::new({
            let persisted = persisted.clone();
            let persist_attempts = persist_attempts.clone();
            move |changes| {
                let persisted = persisted.clone();
                let persist_attempts = persist_attempts.clone();
                Box::pin(async move {
                    persist_attempts.fetch_add(1, Ordering::SeqCst);
                    if changes.iter().any(|(key, value)| {
                        key == "TASK_POOL_SIZE" && value.as_deref() == Some("4")
                    }) {
                        return Err("reject task pool size update".to_string());
                    }

                    let mut persisted = persisted.lock().expect("fake settings store should lock");
                    for (key, value) in changes {
                        if let Some(value) = value {
                            persisted.insert(key, Some(value));
                        } else {
                            persisted.remove(&key);
                        }
                    }
                    Ok(())
                })
            }
        }),
    )
}

fn test_library_catalog_operations() -> LibraryCatalogOperations {
    LibraryCatalogOperations {
        list_libraries: Arc::new(|_context: DiscoveryQueryContext| {
            Box::pin(async {
                panic!("list_libraries should not be called in server settings tests")
            })
        }),
        get_library: Arc::new(|_context: DiscoveryQueryContext, _library_id: String| {
            Box::pin(async { panic!("get_library should not be called in server settings tests") })
        }),
        create_library: Arc::new(|_changes| {
            Box::pin(async {
                panic!("create_library should not be called in server settings tests")
            })
        }),
        update_library: Arc::new(|_library_id, _changes| {
            Box::pin(async {
                panic!("update_library should not be called in server settings tests")
            })
        }),
        delete_library: Arc::new(|_library_id| {
            Box::pin(async {
                panic!("delete_library should not be called in server settings tests")
            })
        }),
        scan_library: Arc::new(|_library_id, _deep_scan| {
            Box::pin(async { panic!("scan_library should not be called in server settings tests") })
        }),
        analyze_library: Arc::new(|_library_id| {
            Box::pin(async {
                panic!("analyze_library should not be called in server settings tests")
            })
        }),
        refresh_metadata: Arc::new(|_library_id| {
            Box::pin(async {
                panic!("refresh_metadata should not be called in server settings tests")
            })
        }),
        empty_trash: Arc::new(|_library_id| {
            Box::pin(async { panic!("empty_trash should not be called in server settings tests") })
        }),
    }
}

fn admin_headers(fixture_root: &Path) -> HeaderMap {
    let namespace = configure_remember_me_store(fixture_root);
    let user = AuthUser {
        id: "admin-user".to_string(),
        email: "admin@example.org".to_string(),
        password: String::new(),
        roles: vec!["ADMIN".to_string()],
        shared_all_libraries: true,
        shared_library_ids: Vec::new(),
        labels_allow: Vec::new(),
        labels_exclude: Vec::new(),
        age_restriction: None,
    };
    let token = session_token_for_user_with_namespace(&user, &namespace);
    let mut headers = HeaderMap::new();
    headers.insert(
        header::HeaderName::from_static("x-auth-token"),
        HeaderValue::from_str(&token).expect("auth token header should be valid"),
    );
    headers
}

fn unique_fixture_root(case_name: &str) -> PathBuf {
    let unique_suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("komga-rust-{case_name}-{unique_suffix}"))
}
