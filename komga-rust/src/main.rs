#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    komga_server::bootstrap::run_process().await;
}
