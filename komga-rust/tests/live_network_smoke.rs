use axum::http::StatusCode;
use reqwest::Client;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[tokio::test]
async fn live_smoke_server_handles_auth_and_cache_headers() {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
        .await
        .expect("bind test listener");
    let address = listener.local_addr().expect("listener address");
    let server = tokio::spawn(async move {
        komga_rust::app::serve(listener)
            .await
            .expect("server should run");
    });

    let client = Client::new();
    let base_url = format!("http://{address}");

    let me = client
        .get(format!("{base_url}/api/v2/users/me"))
        .header(
            reqwest::header::AUTHORIZATION,
            "Basic dXNlckBleGFtcGxlLm9yZzp1c2Vy",
        )
        .header("X-Auth-Token", "")
        .send()
        .await
        .expect("users/me request");
    assert_eq!(me.status(), StatusCode::OK);
    let token = me
        .headers()
        .get("x-auth-token")
        .expect("token header")
        .to_str()
        .expect("token header utf-8")
        .to_string();
    assert!(!token.is_empty());

    let unauthorized = client
        .get(format!("{base_url}/api/v1/libraries"))
        .send()
        .await
        .expect("libraries request without token");
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

    let authorized = client
        .get(format!("{base_url}/api/v1/libraries"))
        .header("X-Auth-Token", &token)
        .send()
        .await
        .expect("libraries request with token");
    assert_eq!(authorized.status(), StatusCode::OK);

    let page = client
        .get(format!("{base_url}/api/v1/books/book-1/pages/1"))
        .header("X-Auth-Token", &token)
        .send()
        .await
        .expect("book page request");
    assert_eq!(page.status(), StatusCode::OK);
    let last_modified = page
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .expect("last-modified header")
        .clone();

    let cached = client
        .get(format!("{base_url}/api/v1/books/book-1/pages/1"))
        .header("X-Auth-Token", &token)
        .header(reqwest::header::IF_MODIFIED_SINCE, last_modified)
        .send()
        .await
        .expect("cached book page request");
    assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);

    server.abort();
    let _ = server.await;
}
