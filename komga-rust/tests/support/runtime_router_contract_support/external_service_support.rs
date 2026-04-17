use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha512};
use sqlx::Row;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::RuntimeDbPaths;

pub(crate) struct SingleResponseServer {
    pub(crate) url: String,
    pub(crate) join: tokio::task::JoinHandle<()>,
}

pub(crate) fn kobo_proxy_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn restore_env_var(key: &str, value: Option<String>) {
    if let Some(value) = value {
        unsafe {
            std::env::set_var(key, value);
        }
    } else {
        unsafe {
            std::env::remove_var(key);
        }
    }
}

pub(crate) async fn spawn_single_response_server(
    status_code: u16,
    content_type: &str,
    body: &str,
) -> SingleResponseServer {
    spawn_single_response_server_with_headers(status_code, content_type, body, &[]).await
}

pub(crate) async fn spawn_single_response_server_with_headers(
    status_code: u16,
    content_type: &str,
    body: &str,
    headers: &[(&str, &str)],
) -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock response server should bind");
    let address = listener
        .local_addr()
        .expect("mock response server should have local addr");
    let body = body.to_string();
    let content_type = content_type.to_string();
    let headers = headers
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect::<Vec<_>>();
    let join = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("mock response server should accept one connection");
        let mut request = [0_u8; 2048];
        let _ = stream.read(&mut request).await;
        let status_text = match status_code {
            200 => "OK",
            404 => "Not Found",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "OK",
        };
        let extra_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}: {value}\r\n"))
            .collect::<String>();
        let response = format!(
            "HTTP/1.1 {status_code} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n{}",
            body.len(),
            extra_headers,
            body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("mock response server should write response");
    });

    SingleResponseServer {
        url: format!("http://{address}/feed.json"),
        join,
    }
}

pub(crate) async fn spawn_request_body_echo_server() -> SingleResponseServer {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("mock request echo server should bind");
    let address = listener
        .local_addr()
        .expect("mock request echo server should have local addr");
    let join = tokio::spawn(async move {
        let (mut stream, _) = listener
            .accept()
            .await
            .expect("mock request echo server should accept one connection");
        let mut request = Vec::new();
        let mut chunk = [0_u8; 1024];

        loop {
            let read = stream
                .read(&mut chunk)
                .await
                .expect("mock request echo server should read request bytes");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);

            let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    line.split_once(':').and_then(|(name, value)| {
                        name.trim()
                            .eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())
                            .flatten()
                    })
                })
                .unwrap_or(0);
            let body_start = header_end + 4;
            if request.len() >= body_start + content_length {
                break;
            }
        }

        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("mock request echo server should receive complete headers");
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let request_target = headers
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let request_query = request_target
            .split_once('?')
            .map(|(_, query)| query)
            .unwrap_or("");
        let content_length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':').and_then(|(name, value)| {
                    name.trim()
                        .eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
            })
            .unwrap_or(0);
        let body_start = header_end + 4;
        let body_end = body_start + content_length;
        let body = String::from_utf8_lossy(&request[body_start..body_end]).to_string();
        let response_body = serde_json::to_string(&serde_json::json!({
            "received": body,
            "query": request_query
        }))
        .expect("mock request echo payload should serialize");
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            response_body.len(),
            response_body
        );
        stream
            .write_all(response.as_bytes())
            .await
            .expect("mock request echo server should write response");
    });

    SingleResponseServer {
        url: format!("http://{address}/echo.json"),
        join,
    }
}

pub(crate) async fn upsert_server_setting(paths: &RuntimeDbPaths, key: &str, value: &str) {
    let pool = komga_infrastructure::sqlite::connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("server settings db should open");

    sqlx::query("INSERT OR REPLACE INTO SERVER_SETTINGS (KEY, VALUE) VALUES (?, ?)")
        .bind(key)
        .bind(value)
        .execute(&pool)
        .await
        .expect("server setting should upsert");

    pool.close().await;
}

pub(crate) async fn load_server_setting(paths: &RuntimeDbPaths, key: &str) -> Option<String> {
    let pool = komga_infrastructure::sqlite::connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("server settings read db should open");

    let row = sqlx::query("SELECT VALUE FROM SERVER_SETTINGS WHERE KEY = ?")
        .bind(key)
        .fetch_optional(&pool)
        .await
        .expect("server setting should load");

    pool.close().await;
    row.map(|row| row.get::<String, _>("VALUE"))
}

pub(crate) async fn seed_kobo_sync_api_key(paths: &RuntimeDbPaths, api_key: &str, user_id: &str) {
    let pool = komga_infrastructure::sqlite::connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("user api key seed db should open");

    let api_key_hash = {
        let mut hasher = Sha512::new();
        hasher.update(api_key.as_bytes());
        hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };

    sqlx::query("INSERT INTO USER_API_KEY (ID, USER_ID, API_KEY, COMMENT) VALUES (?, ?, ?, ?)")
        .bind(format!("api-key-{api_key}"))
        .bind(user_id)
        .bind(api_key_hash)
        .bind("kobo sync")
        .execute(&pool)
        .await
        .expect("user api key row should be inserted");

    pool.close().await;
}
