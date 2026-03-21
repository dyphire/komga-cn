#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = komga_rust::config::RuntimeConfig::from_env().expect("invalid runtime config");

    let listener = tokio::net::TcpListener::bind(config.bind_address)
        .await
        .expect("failed to bind address");

    komga_rust::app::serve_with_config(listener, config)
        .await
        .expect("server error");
}
