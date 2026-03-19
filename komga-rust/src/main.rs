use std::net::SocketAddr;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let address = std::env::var("KOMGA_RUST_ADDR").unwrap_or_else(|_| "127.0.0.1:3000".to_string());
    let address: SocketAddr = address.parse().expect("invalid KOMGA_RUST_ADDR");

    let listener = tokio::net::TcpListener::bind(address)
        .await
        .expect("failed to bind address");

    komga_rust::app::serve(listener).await.expect("server error");
}
