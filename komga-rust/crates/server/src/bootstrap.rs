use tokio::net::TcpListener;

use crate::composition::start_server;
use crate::config::RuntimeConfig;

#[path = "bootstrap/admin_cli.rs"]
pub mod admin_cli;
#[path = "bootstrap/noclaim_bootstrap.rs"]
pub mod noclaim_bootstrap;

pub async fn run_process() {
    let admin_commands = admin_cli::parse_admin_cli_commands(std::env::args().skip(1));

    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    validate_startup_schema_gate(&config).await;
    noclaim_bootstrap::ensure_noclaim_initial_users(&config).await;
    admin_cli::run_admin_cli_commands(&config, &admin_commands).await;

    let listener = TcpListener::bind(config.bind_address)
        .await
        .expect("failed to bind address");

    start_server::serve_with_config(listener, config)
        .await
        .expect("server error");
}

async fn validate_startup_schema_gate(config: &RuntimeConfig) {
    let main_pool = komga_infrastructure::sqlite::connect_pool(&config.database_file, 1)
        .await
        .expect("failed to open main sqlite database");
    let main_schema_result = komga_infrastructure::sqlite::setup::bootstrap_pool(&main_pool).await;
    main_pool.close().await;
    main_schema_result.expect("main sqlite schema gate failed");

    let tasks_pool = komga_infrastructure::sqlite::connect_pool(&config.tasks_db_file, 1)
        .await
        .expect("failed to open tasks sqlite database");
    let tasks_schema_result =
        komga_infrastructure::sqlite::setup::bootstrap_tasks_pool(&tasks_pool).await;
    tasks_pool.close().await;
    tasks_schema_result.expect("tasks sqlite schema gate failed");
}
