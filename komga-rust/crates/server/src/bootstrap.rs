use tokio::net::TcpListener;

use komga_application::task_processing::TaskRuntimeConfig;

use crate::build_metadata::current_build_metadata;
use crate::composition::start_server;
use crate::config::{RuntimeConfig, RuntimeMode, RuntimeProfile, WriterDecision, WriterKind};

#[path = "bootstrap/admin_cli.rs"]
pub mod admin_cli;
#[path = "bootstrap/noclaim_bootstrap.rs"]
pub mod noclaim_bootstrap;

const PRODUCT_NAME: &str = "komga-rust";
const STARTUP_BANNER_TEMPLATE: &str =
    include_str!("../../../../komga/src/main/resources/banner.txt");
const APPLICATION_VERSION_PLACEHOLDER: &str = "${application.version}";

pub(crate) fn emit_startup_banner_and_runtime_event(config: &RuntimeConfig) {
    let build = current_build_metadata();
    let rendered_banner = render_startup_banner(build.version.as_str());
    let _ = crate::logging::emit_display(rendered_banner.as_str());
    let task_runtime = config.task_runtime_context();
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
        consumes_queue = task_runtime.consumes_queue,
        owns_main_database = task_runtime.owns_main_database,
        owns_filesystem_scan_output = task_runtime.owns_filesystem_scan_output,
        owns_sidecar_output = task_runtime.owns_sidecar_output,
        owns_search_index = task_runtime.owns_search_index,
        "Resolved startup runtime identity",
    );
}

pub async fn run_process() {
    match admin_cli::parse_startup_cli(std::env::args().skip(1)) {
        Ok(admin_cli::StartupCliPreflight::Help) => {
            println!("{}", admin_cli::render_usage());
        }
        Ok(admin_cli::StartupCliPreflight::Admin(commands)) => run_admin_action(commands).await,
        Ok(admin_cli::StartupCliPreflight::Server) => run_server().await,
        Err(error) => {
            eprintln!("{}\n\n{}", error, admin_cli::render_usage());
            std::process::exit(2);
        }
    }
}

async fn run_admin_action(commands: admin_cli::AdminCliCommands) {
    let config = crate::config::AdminActionConfig::from_env().unwrap_or_else(|error| {
        eprintln!("failed to resolve admin action config: {error}");
        std::process::exit(1);
    });

    validate_main_startup_schema_gate(config.database_file.as_path())
        .await
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });

    admin_cli::run_admin_cli_commands(config.database_file.as_path(), &commands)
        .await
        .unwrap_or_else(|error| {
            eprintln!("{error}");
            std::process::exit(1);
        });
}

async fn run_server() {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    crate::logging::init_global(&config).expect("failed to initialize logging");
    validate_startup_schema_gate(&config)
        .await
        .expect("startup schema gate failed");
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

    start_server::serve_with_config(listener, config)
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

    let main_pool = komga_infrastructure::sqlite::connect_pool(database_file, 1)
        .await
        .map_err(|error| {
            schema_gate_failure(
                "main",
                database_file,
                "failed to open main sqlite database",
                error,
            )
        })?;
    let main_schema_result = komga_infrastructure::sqlite::setup::bootstrap_pool(&main_pool).await;
    main_pool.close().await;
    main_schema_result.map_err(|error| {
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

    let tasks_pool = komga_infrastructure::sqlite::connect_pool(&config.tasks_db_file, 1)
        .await
        .map_err(|error| {
            schema_gate_failure(
                "tasks",
                &config.tasks_db_file,
                "failed to open tasks sqlite database",
                error,
            )
        })?;
    let tasks_schema_result =
        komga_infrastructure::sqlite::setup::bootstrap_tasks_pool(&tasks_pool).await;
    tasks_pool.close().await;
    tasks_schema_result.map_err(|error| {
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
