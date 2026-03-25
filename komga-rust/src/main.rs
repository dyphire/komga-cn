#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = komga_rust::config::RuntimeConfig::from_env().expect("invalid runtime config");
    validate_startup_schema_gate(&config).await;

    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .expect("failed to bind address");

    komga_rust::app::serve_with_config(listener, config)
        .await
        .expect("server error");
}

async fn validate_startup_schema_gate(config: &komga_rust::config::RuntimeConfig) {
    let main_pool = komga_rust::persistence::sqlite::connect_pool(&config.database_file, 1)
        .await
        .expect("failed to open main sqlite database");
    let main_schema_result =
        komga_rust::persistence::sqlite::setup::bootstrap_pool(&main_pool).await;
    main_pool.close().await;
    main_schema_result.expect("main sqlite schema gate failed");

    let tasks_pool = komga_rust::persistence::sqlite::connect_pool(&config.tasks_db_file, 1)
        .await
        .expect("failed to open tasks sqlite database");
    let tasks_schema_result =
        komga_rust::persistence::sqlite::setup::bootstrap_tasks_pool(&tasks_pool).await;
    tasks_pool.close().await;
    tasks_schema_result.expect("tasks sqlite schema gate failed");
}
