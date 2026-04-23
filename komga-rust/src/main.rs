#[tokio::main]
async fn main() {
    komga_server::bootstrap::run_process().await;
}
