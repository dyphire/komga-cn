use std::collections::BTreeMap;
use std::time::Instant;
use tokio::net::TcpListener;

use crate::build_metadata::current_build_metadata;
use komga_config::cli_args::RuntimeCli;
use komga_config::env_config::{RuntimeConfig, RuntimeDatabaseSettings};
use komga_config::profile::{RuntimeMode, RuntimeProfile};
use komga_config::writer_ownership::{WriterDecision, WriterKind};
use komga_infrastructure::operational_access::load_server_settings;
use komga_infrastructure::sqlite::write_models::server_settings::ServerSettingsStore;
use komga_interfaces::state::StartupTimingState;

pub mod admin_cli;
pub mod noclaim_bootstrap;

const PRODUCT_NAME: &str = "komga-rust";
const STARTUP_BANNER_TEMPLATE: &str =
    include_str!("../../../../../komga/src/main/resources/banner.txt");
const APPLICATION_VERSION_PLACEHOLDER: &str = "${application.version}";

pub(crate) async fn emit_startup_banner_and_runtime_event(config: &RuntimeConfig) {
    let build = current_build_metadata();
    let rendered_banner = render_startup_banner(build.version.as_str());
    let _ = crate::logging::emit_display(rendered_banner.as_str());
    let task_runtime = crate::config::task_runtime_context(config).await;
    let config_dir = config
        .config_dir
        .as_ref()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_default();
    let main_database_writer_decision = config.writer_decision(WriterKind::MainDatabase);
    let tasks_database_writer_decision = config.writer_decision(WriterKind::TasksDatabase);
    let filesystem_scan_writer_decision = config.writer_decision(WriterKind::FilesystemScanOutput);
    let sidecar_writer_decision = config.writer_decision(WriterKind::SidecarOutput);
    let search_writer_decision = config.writer_decision(WriterKind::SearchIndex);

    tracing::info!(
        event = "startup_banner",
        product = PRODUCT_NAME,
        version = build.version.as_str(),
        build_time = build.build_time.as_str(),
        git_branch = build.git_branch.as_deref().unwrap_or_default(),
        git_commit_id = build.git_commit_id.as_deref().unwrap_or_default(),
        git_commit_time = build.git_commit_time.as_deref().unwrap_or_default(),
        runtime_mode = runtime_mode_label(config.mode),
        runtime_profile = runtime_profile_label(config.runtime_profile),
        bind_address = %config.bind_address,
        "Komga Rust startup",
    );

    tracing::info!(
        event = "startup_runtime",
        runtime_mode = runtime_mode_label(config.mode),
        runtime_profile = runtime_profile_label(config.runtime_profile),
        bind_address = %config.bind_address,
        config_dir = config_dir.as_str(),
        database_file = %config.database_file.display(),
        tasks_db_file = %config.tasks_db_file.display(),
        lucene_data_directory = %config.lucene_data_directory.display(),
        fonts_data_directory = %config.fonts_data_directory.display(),
        main_database_writer_decision = writer_decision_label(main_database_writer_decision),
        main_database_writer_reason = writer_decision_reason(main_database_writer_decision),
        tasks_database_writer_decision = writer_decision_label(tasks_database_writer_decision),
        tasks_database_writer_reason = writer_decision_reason(tasks_database_writer_decision),
        filesystem_scan_writer_decision = writer_decision_label(filesystem_scan_writer_decision),
        filesystem_scan_writer_reason = writer_decision_reason(filesystem_scan_writer_decision),
        sidecar_writer_decision = writer_decision_label(sidecar_writer_decision),
        sidecar_writer_reason = writer_decision_reason(sidecar_writer_decision),
        search_writer_decision = writer_decision_label(search_writer_decision),
        search_writer_reason = writer_decision_reason(search_writer_decision),
        consumes_queue = task_runtime.worker().consumes_queue(),
        owns_main_database = task_runtime.job().database().owns_main_database(),
        owns_filesystem_scan_output = task_runtime.job().filesystem().owns_filesystem_scan_output(),
        owns_sidecar_output = task_runtime.job().filesystem().owns_sidecar_output(),
        owns_search_index = task_runtime.job().search().owns_search_index(),
        "Resolved startup runtime identity",
    );
}

pub async fn run_process() {
    let startup_started_at = Instant::now();
    match admin_cli::parse_startup_cli(std::env::args().skip(1)) {
        Ok(admin_cli::StartupCliPreflight::Help) => {
            println!("{}", admin_cli::render_usage());
        }
        Ok(admin_cli::StartupCliPreflight::Admin(commands)) => run_admin_action(commands).await,
        Ok(admin_cli::StartupCliPreflight::Server) => run_server(startup_started_at).await,
        Err(error) => {
            eprintln!("{}\n\n{}", error, admin_cli::render_usage());
            std::process::exit(2);
        }
    }
}

async fn run_admin_action(commands: admin_cli::AdminCliCommands) {
    let config = komga_config::env_config::AdminActionConfig::from_env().unwrap_or_else(|error| {
        eprintln!("failed to resolve admin action config: {error}");
        std::process::exit(1);
    });

    ensure_existing_admin_database(config.database_file()).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });

    validate_main_startup_schema_gate(config.database_file())
        .await
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });

    admin_cli::run_admin_cli_commands(config.database_file(), &commands)
        .await
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });
}

fn ensure_existing_admin_database(database_file: &std::path::Path) -> std::io::Result<()> {
    match std::fs::metadata(database_file) {
        Ok(metadata) if metadata.is_file() => Ok(()),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "admin action requires an existing main database file at {}",
                database_file.display()
            ),
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!(
                "admin action requires an existing main database at {}",
                database_file.display()
            ),
        )),
        Err(error) => Err(std::io::Error::new(
            error.kind(),
            format!(
                "failed to inspect admin action database {}: {error}",
                database_file.display()
            ),
        )),
    }
}

async fn run_server(startup_started_at: Instant) {
    let base_config = RuntimeConfig::from_env().expect("invalid runtime config");
    crate::logging::init_global(&base_config).expect("failed to initialize logging");
    validate_startup_schema_gate(&base_config)
        .await
        .expect("startup schema gate failed");
    let config = resolve_runtime_config_with_database(base_config)
        .await
        .expect("failed to merge persisted runtime settings");
    noclaim_bootstrap::ensure_noclaim_initial_users(&config).await;

    let listener = TcpListener::bind(config.bind_address)
        .await
        .map_err(|error| {
            tracing::error!(
                event = "server_bind",
                outcome = "failed",
                bind_address = %config.bind_address,
                error = %error,
                "Server listener bind failed",
            );
            error
        })
        .expect("failed to bind address");

    crate::app::serve_with_startup_timing(
        listener,
        config,
        StartupTimingState::default(),
        startup_started_at,
    )
    .await
    .expect("server error");
}

pub(crate) async fn validate_startup_schema_gate(config: &RuntimeConfig) -> std::io::Result<()> {
    validate_main_startup_schema_gate(config.database_file.as_path()).await?;
    validate_tasks_startup_schema_gate(config).await?;
    Ok(())
}

async fn validate_main_startup_schema_gate(database_file: &std::path::Path) -> std::io::Result<()> {
    tracing::info!(
        event = "startup_schema_gate",
        database_role = "main",
        outcome = "checking",
        database_file = %database_file.display(),
        "Checking main sqlite schema gate",
    );

    let main_pool = komga_infrastructure::sqlite::connect_read_pool(database_file)
        .await
        .map_err(|error| {
            schema_gate_failure(
                "main",
                database_file,
                "failed to open main sqlite database",
                error,
            )
        })?;
    komga_infrastructure::sqlite::setup::bootstrap_pool(&main_pool)
        .await
        .map_err(|error| {
            schema_gate_failure(
                "main",
                database_file,
                "main sqlite schema gate failed",
                error,
            )
        })?;

    tracing::info!(
        event = "startup_schema_gate",
        database_role = "main",
        outcome = "ready",
        database_file = %database_file.display(),
        "Main sqlite schema gate ready",
    );

    Ok(())
}

async fn validate_tasks_startup_schema_gate(config: &RuntimeConfig) -> std::io::Result<()> {
    tracing::info!(
        event = "startup_schema_gate",
        database_role = "tasks",
        outcome = "checking",
        database_file = %config.tasks_db_file.display(),
        "Checking tasks sqlite schema gate",
    );

    let tasks_pool = komga_infrastructure::sqlite::connect_read_pool(&config.tasks_db_file)
        .await
        .map_err(|error| {
            schema_gate_failure(
                "tasks",
                &config.tasks_db_file,
                "failed to open tasks sqlite database",
                error,
            )
        })?;
    komga_infrastructure::sqlite::setup::bootstrap_tasks_pool(&tasks_pool)
        .await
        .map_err(|error| {
            schema_gate_failure(
                "tasks",
                &config.tasks_db_file,
                "tasks sqlite schema gate failed",
                error,
            )
        })?;

    tracing::info!(
        event = "startup_schema_gate",
        database_role = "tasks",
        outcome = "ready",
        database_file = %config.tasks_db_file.display(),
        "Tasks sqlite schema gate ready",
    );

    Ok(())
}

fn schema_gate_failure(
    database_role: &'static str,
    database_file: &std::path::Path,
    context: &'static str,
    error: impl std::fmt::Display,
) -> std::io::Error {
    let error_message = format!("{context}: {error}");
    tracing::error!(
        event = "startup_schema_gate",
        database_role,
        outcome = "failed",
        database_file = %database_file.display(),
        error = error_message.as_str(),
        "Startup schema gate failed",
    );
    std::io::Error::other(error_message)
}

pub(crate) async fn resolve_runtime_config_with_database(
    base_config: RuntimeConfig,
) -> std::io::Result<RuntimeConfig> {
    let database_settings = load_runtime_database_settings(&base_config).await?;
    let cli = RuntimeCli::default();
    let env = std::env::vars().collect::<BTreeMap<_, _>>();

    RuntimeConfig::resolve_with_env_and_database(&cli, &env, database_settings)
        .map_err(|error| std::io::Error::other(format!("invalid runtime config: {error}")))
}

async fn load_runtime_database_settings(
    config: &RuntimeConfig,
) -> std::io::Result<RuntimeDatabaseSettings> {
    let store = ServerSettingsStore::new(config.database_file.clone());
    let settings = load_server_settings(&store)
        .await
        .map_err(|error| std::io::Error::other(format!("load server settings: {error}")))?;

    if let Some(server_context_path) = settings.server_context_path.as_deref()
        && !server_context_path.is_empty()
        && !is_valid_persisted_server_context_path(server_context_path)
    {
        tracing::warn!(
            event = "startup_config",
            setting = "SERVER_CONTEXT_PATH",
            value = server_context_path,
            outcome = "ignored",
            "Ignoring invalid persisted server context path",
        );
    }

    Ok(RuntimeDatabaseSettings {
        server_port: settings.server_port,
        server_context_path: settings.server_context_path,
        task_pool_size: Some(settings.task_pool_size as usize),
    })
}

fn is_valid_persisted_server_context_path(value: &str) -> bool {
    if value.ends_with('/') {
        return false;
    }

    value
        .chars()
        .all(|ch| ch == '/' || ch == '-' || ch == '_' || ch.is_ascii_alphanumeric())
        && value.starts_with('/')
}

fn render_startup_banner(version: &str) -> String {
    let rendered = STARTUP_BANNER_TEMPLATE.replace(APPLICATION_VERSION_PLACEHOLDER, version);
    if rendered.ends_with('\n') {
        rendered
    } else {
        format!("{rendered}\n")
    }
}

fn runtime_mode_label(mode: RuntimeMode) -> &'static str {
    match mode {
        RuntimeMode::Snapshot => "snapshot",
        RuntimeMode::Localdb => "localdb",
        RuntimeMode::Isolated => "isolated",
        RuntimeMode::Canary => "canary",
    }
}

fn runtime_profile_label(profile: RuntimeProfile) -> &'static str {
    match profile {
        RuntimeProfile::SnapshotAligned => "snapshot-aligned",
        RuntimeProfile::LiveLocaldb => "live-localdb",
    }
}

fn writer_decision_label(decision: WriterDecision) -> &'static str {
    match decision {
        WriterDecision::Allowed => "allowed",
        WriterDecision::Isolated => "isolated",
        WriterDecision::Blocked { .. } => "blocked",
    }
}

fn writer_decision_reason(decision: WriterDecision) -> &'static str {
    match decision {
        WriterDecision::Allowed | WriterDecision::Isolated => "",
        WriterDecision::Blocked { reason } => reason,
    }
}
