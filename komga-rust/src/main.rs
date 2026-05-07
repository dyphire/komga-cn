use mimalloc::MiMalloc;

#[global_allocator]
static GLOBAL_ALLOCATOR: MiMalloc = MiMalloc;

#[tokio::main]
async fn main() {
    komga_server::bootstrap::run_process().await;
}
