use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::PathBuf;

#[test]
fn runtime_profile_startup_contract_defaults_match_packaged_contract() {
    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &BTreeMap::new(),
    )
    .expect("config should resolve");

    assert_eq!(
        config.bind_address,
        "127.0.0.1:25600"
            .parse::<SocketAddr>()
            .expect("valid socket address"),
    );
    assert_eq!(config.mode, komga_rust::config::RuntimeMode::Snapshot);
    assert_eq!(
        config.compat_profile,
        komga_rust::config::CompatProfile::SnapshotAligned,
    );
    assert_eq!(config.config_dir, Some(PathBuf::from(".komga")));
    assert_eq!(config.server_context_path.as_deref(), Some(""));
    assert_eq!(
        config.log_file,
        PathBuf::from(".komga").join("logs").join("komga.log"),
    );
    assert_eq!(config.kepubify_path, None);
}

#[test]
fn config_precedence_is_deterministic() {
    let mut env = BTreeMap::new();
    env.insert("KOMGA_RUST_ADDR".to_string(), "127.0.0.1:4001".to_string());
    env.insert("KOMGA_RUST_MODE".to_string(), "shadow".to_string());
    env.insert(
        "KOMGA_RUST_COMPAT_PROFILE".to_string(),
        "java-live-localdb".to_string(),
    );

    let cli = komga_rust::config::RuntimeCli {
        address: Some("127.0.0.1:4010".to_string()),
        mode: Some("localdb".to_string()),
        compat_profile: Some("snapshot-aligned".to_string()),
        platform_profile: None,
        config_dir: Some("/tmp/komga-cli".into()),
        log_file: None,
        kepubify_path: None,
        shadow_isolation_root: Some("/tmp/komga-shadow".into()),
        allow_shadow_writes: false,
    };

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(&cli, &env)
        .expect("config should resolve");

    assert_eq!(
        config.bind_address,
        "127.0.0.1:4010"
            .parse::<SocketAddr>()
            .expect("valid socket address"),
    );
    assert_eq!(config.mode, komga_rust::config::RuntimeMode::Localdb);
    assert_eq!(
        config.compat_profile,
        komga_rust::config::CompatProfile::SnapshotAligned,
    );
    assert_eq!(config.config_dir, Some(PathBuf::from("/tmp/komga-cli")));
    assert_eq!(config.server_context_path.as_deref(), Some(""));
    assert_eq!(
        config.log_file,
        PathBuf::from("/tmp/komga-cli")
            .join("logs")
            .join("komga.log"),
    );
    assert_eq!(config.kepubify_path, None);
    assert_eq!(
        config.shadow_policy.isolation_root,
        Some(PathBuf::from("/tmp/komga-shadow")),
    );
    assert!(!config.shadow_policy.allow_shadow_writes);
}

#[test]
fn docker_runtime_contract_uses_config_dir_for_log_path_and_linux_kepubify_binary() {
    let mut env = BTreeMap::new();
    env.insert("KOMGA_RUST_MODE".to_string(), "localdb".to_string());
    env.insert("KOMGA_CONFIG_DIR".to_string(), "/config".to_string());
    env.insert(
        "KOMGA_RUST_COMPAT_PROFILE".to_string(),
        "java-live-localdb".to_string(),
    );
    env.insert(
        "KOMGA_RUST_PLATFORM_PROFILE".to_string(),
        "docker".to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("config should resolve");

    assert_eq!(config.config_dir, Some(PathBuf::from("/config")));
    assert_eq!(
        config.log_file,
        PathBuf::from("/config").join("logs").join("komga.log"),
    );
    assert_eq!(
        config.kepubify_path,
        Some(PathBuf::from("/usr/bin/kepubify"))
    );
    assert_eq!(config.server_context_path.as_deref(), Some(""));
}

#[test]
fn mac_runtime_contract_uses_application_support_and_logs_defaults() {
    let mut env = BTreeMap::new();
    env.insert("HOME".to_string(), "/Users/komga".to_string());
    env.insert("KOMGA_RUST_PLATFORM_PROFILE".to_string(), "mac".to_string());

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("config should resolve");

    assert_eq!(
        config.config_dir,
        Some(PathBuf::from(
            "/Users/komga/Library/Application Support/Komga"
        )),
    );
    assert_eq!(
        config.log_file,
        PathBuf::from("/Users/komga/Library/Logs/Komga/komga.log"),
    );
    assert_eq!(config.kepubify_path, Some(PathBuf::from("kepubify")));
}

#[test]
fn windows_runtime_contract_uses_localappdata_defaults() {
    let mut env = BTreeMap::new();
    env.insert(
        "LOCALAPPDATA".to_string(),
        r#"C:\Users\komga\AppData\Local"#.to_string(),
    );
    env.insert(
        "KOMGA_RUST_PLATFORM_PROFILE".to_string(),
        "windows".to_string(),
    );

    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli::default(),
        &env,
    )
    .expect("config should resolve");

    assert_eq!(
        config.config_dir,
        Some(PathBuf::from(r#"C:\Users\komga\AppData\Local"#).join("Komga")),
    );
    assert_eq!(
        config.log_file,
        PathBuf::from(r#"C:\Users\komga\AppData\Local"#)
            .join("Komga")
            .join("logs")
            .join("komga.log"),
    );
    assert_eq!(config.kepubify_path, Some(PathBuf::from("kepubify.exe")));
}

#[test]
fn shadow_mode_blocks_unsafe_writers() {
    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli {
            mode: Some("shadow".to_string()),
            ..Default::default()
        },
        &BTreeMap::new(),
    )
    .expect("config should resolve");

    for writer in [
        komga_rust::config::WriterKind::MainDatabase,
        komga_rust::config::WriterKind::TasksDatabase,
        komga_rust::config::WriterKind::SearchIndex,
        komga_rust::config::WriterKind::FilesystemScanOutput,
        komga_rust::config::WriterKind::SidecarOutput,
    ] {
        let decision = config.writer_decision(writer);
        let expected_reason = if matches!(writer, komga_rust::config::WriterKind::SearchIndex) {
            "search index ownership remains with java writer in shadow mode"
        } else {
            "shadow mode requires explicit isolation or opt-in"
        };
        assert_eq!(
            decision,
            komga_rust::config::WriterDecision::Blocked {
                reason: expected_reason,
            },
            "writer {writer:?} should be blocked in shadow mode by default",
        );
        assert!(
            !decision.allows_write(),
            "writer {writer:?} must not allow writes in default shadow mode",
        );
    }
}

#[test]
fn shadow_mode_keeps_search_index_owned_by_java_writer_even_when_other_writes_are_isolated() {
    let config = komga_rust::config::RuntimeConfig::resolve_with_env(
        &komga_rust::config::RuntimeCli {
            mode: Some("shadow".to_string()),
            shadow_isolation_root: Some("/tmp/komga-shadow".into()),
            allow_shadow_writes: true,
            ..Default::default()
        },
        &BTreeMap::new(),
    )
    .expect("config should resolve");

    let search_index_decision = config.writer_decision(komga_rust::config::WriterKind::SearchIndex);
    assert_eq!(
        search_index_decision,
        komga_rust::config::WriterDecision::Blocked {
            reason: "search index ownership remains with java writer in shadow mode",
        },
    );
    assert!(
        !search_index_decision.allows_write(),
        "shadow mode must never let rust write shared search index",
    );

    let main_db_decision = config.writer_decision(komga_rust::config::WriterKind::MainDatabase);
    assert_eq!(
        main_db_decision,
        komga_rust::config::WriterDecision::Isolated
    );
    assert!(main_db_decision.allows_write());
}
