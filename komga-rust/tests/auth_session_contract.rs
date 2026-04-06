use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use komga_contract_testkit::contract_matrix::assert_required_target_declared;
use komga_rust::infrastructure::sqlite::connect_pool;
use komga_server::app::build_router_with_config;
use serde_json::{Value, json};
use sha2::{Digest, Sha512};
use sqlx::Row;
use std::sync::{Mutex, OnceLock};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tower::util::ServiceExt;

#[path = "support/runtime_router_contract_support.rs"]
pub mod runtime_router_contract_support;

use runtime_router_contract_support::*;

#[path = "auth_session_contract/kobo_and_session_basics.rs"]
mod kobo_and_session_basics;
#[path = "auth_session_contract/koreader_activity_syncpoints.rs"]
mod koreader_activity_syncpoints;
#[path = "auth_session_contract/page_hash_routes.rs"]
mod page_hash_routes;
#[path = "auth_session_contract/releases_announcements_client_settings.rs"]
mod releases_announcements_client_settings;

#[test]
fn auth_session_contract_target_is_registered() {
    assert_required_target_declared("auth/session", "auth_session_contract");
}

async fn seed_syncpoint_user(paths: &RuntimeDbPaths, user_id: &str, email: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint user db should open");

    sqlx::query(
        "INSERT INTO USER (ID, EMAIL, PASSWORD, SHARED_ALL_LIBRARIES) \
         VALUES (?, ?, '', ?)",
    )
    .bind(user_id)
    .bind(email)
    .bind(true)
    .execute(&pool)
    .await
    .expect("syncpoint test user should be inserted");

    pool.close().await;
}

async fn seed_global_client_setting(
    paths: &RuntimeDbPaths,
    key: &str,
    value: &str,
    allow_unauthorized: bool,
) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("global client settings db should open");

    sqlx::query(
        "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED) VALUES (?, ?, ?)",
    )
    .bind(key)
    .bind(value)
    .bind(allow_unauthorized)
    .execute(&pool)
    .await
    .expect("global client setting row should be inserted");

    pool.close().await;
}

async fn load_announcement_read_ids_for_user(paths: &RuntimeDbPaths, user_id: &str) -> Vec<String> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("announcements read query db should open");

    let rows = sqlx::query(
        "SELECT ANNOUNCEMENT_ID FROM ANNOUNCEMENTS_READ WHERE USER_ID = ? ORDER BY ANNOUNCEMENT_ID",
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await
    .expect("announcement read ids should load");
    pool.close().await;

    rows.into_iter()
        .map(|row| row.get::<String, _>("ANNOUNCEMENT_ID"))
        .collect()
}

fn releases_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn announcements_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn kobo_proxy_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn restore_env_var(key: &str, value: Option<String>) {
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

struct SingleResponseServer {
    url: String,
    join: tokio::task::JoinHandle<()>,
}

async fn spawn_single_response_server(
    status_code: u16,
    content_type: &str,
    body: &str,
) -> SingleResponseServer {
    spawn_single_response_server_with_headers(status_code, content_type, body, &[]).await
}

async fn spawn_single_response_server_with_headers(
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

async fn spawn_request_body_echo_server() -> SingleResponseServer {
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
        let response_body =
            serde_json::to_string(&json!({ "received": body, "query": request_query }))
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

async fn seed_announcement_read_ids(paths: &RuntimeDbPaths, user_id: &str, ids: &[&str]) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("announcements seed db should open");

    for id in ids {
        sqlx::query("INSERT INTO ANNOUNCEMENTS_READ (USER_ID, ANNOUNCEMENT_ID) VALUES (?, ?)")
            .bind(user_id)
            .bind(id)
            .execute(&pool)
            .await
            .expect("announcement read row should be inserted");
    }

    pool.close().await;
}

async fn upsert_server_setting(paths: &RuntimeDbPaths, key: &str, value: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

async fn load_page_hash_size(paths: &RuntimeDbPaths, hash: &str) -> Option<i64> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash query db should open");

    let row = sqlx::query("SELECT SIZE FROM PAGE_HASH WHERE HASH = ?")
        .bind(hash)
        .fetch_one(&pool)
        .await
        .expect("page hash row should load");
    pool.close().await;

    row.get::<Option<i64>, _>("SIZE")
}

async fn load_page_hash_record(
    paths: &RuntimeDbPaths,
    hash: &str,
) -> Option<(Option<i64>, String)> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash record query db should open");

    let row = sqlx::query("SELECT SIZE, ACTION FROM PAGE_HASH WHERE HASH = ?")
        .bind(hash)
        .fetch_optional(&pool)
        .await
        .expect("page hash record should be queryable");
    pool.close().await;

    row.map(|row| {
        (
            row.get::<Option<i64>, _>("SIZE"),
            row.get::<String, _>("ACTION"),
        )
    })
}

async fn seed_page_hash_row(paths: &RuntimeDbPaths, hash: &str, size: Option<i64>, action: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash seed db should open");

    sqlx::query("INSERT INTO PAGE_HASH (HASH, SIZE, ACTION, DELETE_COUNT, CREATED_DATE, LAST_MODIFIED_DATE) VALUES (?, ?, ?, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP)")
        .bind(hash)
        .bind(size)
        .bind(action)
        .execute(&pool)
        .await
        .expect("page hash row should be inserted");

    pool.close().await;
}

async fn seed_page_hash_image_source(
    paths: &RuntimeDbPaths,
    book_id: &str,
    hash: &str,
    relative_book_path: &str,
    file_name: &str,
) -> std::path::PathBuf {
    let source_path = paths.config_dir.join(relative_book_path);
    if let Some(parent) = source_path.parent() {
        std::fs::create_dir_all(parent).expect("page hash source parent should be created");
    }

    let image_bytes = fixture_png_bytes();
    std::fs::write(&source_path, &image_bytes).expect("page hash source image should be written");

    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash image source db should open");

    sqlx::query(
        "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(file_name)
    .bind(relative_book_path)
    .bind("series-1")
    .bind(i64::try_from(image_bytes.len()).expect("image bytes length should fit i64"))
    .bind(2_i64)
    .bind("library-1")
    .execute(&pool)
    .await
    .expect("page hash source book row should be inserted");

    sqlx::query(
        "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(book_id)
    .bind(0_i64)
    .bind(hash)
    .bind(file_name)
    .bind("image/png")
    .bind(i64::try_from(image_bytes.len()).expect("image bytes length should fit i64"))
    .execute(&pool)
    .await
    .expect("page hash source media page row should be inserted");

    pool.close().await;
    source_path
}

async fn seed_unknown_page_hash_samples(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("unknown page hash sample db should open");

    for (book_id, name, url, number) in [
        (
            "book-unknown-1",
            "book-unknown-1.epub",
            "books/book-unknown-1.epub",
            10_i64,
        ),
        (
            "book-unknown-2",
            "book-unknown-2.epub",
            "books/book-unknown-2.epub",
            11_i64,
        ),
        (
            "book-unknown-3",
            "book-unknown-3.epub",
            "books/book-unknown-3.epub",
            12_i64,
        ),
        (
            "book-unknown-4",
            "book-unknown-4.epub",
            "books/book-unknown-4.epub",
            13_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("unknown page hash sample book row should be inserted");
    }

    for (book_id, page_hash, file_size) in [
        ("book-unknown-1", "a-hash", 111_i64),
        ("book-unknown-2", "a-hash", 111_i64),
        ("book-unknown-3", "z-hash", 222_i64),
        ("book-unknown-4", "z-hash", 222_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(page_hash)
        .bind(format!("{book_id}.png"))
        .bind("image/png")
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("unknown page hash sample media page row should be inserted");
    }

    pool.close().await;
}

async fn seed_unknown_page_hash_samples_with_mixed_sizes(paths: &RuntimeDbPaths) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("mixed-size unknown page hash sample db should open");

    for (book_id, name, url, number) in [
        (
            "book-mixed-1",
            "book-mixed-1.epub",
            "books/book-mixed-1.epub",
            30_i64,
        ),
        (
            "book-mixed-2",
            "book-mixed-2.epub",
            "books/book-mixed-2.epub",
            31_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("mixed-size unknown page hash sample book row should be inserted");
    }

    for (book_id, file_size) in [("book-mixed-1", 111_i64), ("book-mixed-2", 222_i64)] {
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind("mixed-size-hash")
        .bind(format!("{book_id}.png"))
        .bind("image/png")
        .bind(file_size)
        .execute(&pool)
        .await
        .expect("mixed-size unknown page hash sample media page row should be inserted");
    }

    pool.close().await;
}

async fn seed_page_hash_match_samples(paths: &RuntimeDbPaths, hash: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("page hash match sample db should open");

    for (book_id, name, url, number) in [
        (
            "book-match-1",
            "book-match-1.epub",
            "file:/library-root/books/book-match-1.epub",
            20_i64,
        ),
        (
            "book-match-2",
            "book-match-2.epub",
            "file:/library-root/books/book-match-2.epub",
            21_i64,
        ),
        (
            "book-match-3",
            "book-match-3.epub",
            "file:/library-root/books/book-match-3.epub",
            22_i64,
        ),
    ] {
        sqlx::query(
            "INSERT INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, NUMBER, LIBRARY_ID) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(0_i64)
        .bind(name)
        .bind(url)
        .bind("series-1")
        .bind(2_048_i64)
        .bind(number)
        .bind("library-1")
        .execute(&pool)
        .await
        .expect("page hash match sample book row should be inserted");
    }

    for (book_id, page_number) in [
        ("book-match-1", 0_i64),
        ("book-match-2", 2_i64),
        ("book-match-3", 4_i64),
    ] {
        sqlx::query(
            "INSERT INTO MEDIA_PAGE (BOOK_ID, NUMBER, FILE_HASH, FILE_NAME, MEDIA_TYPE, FILE_SIZE) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(book_id)
        .bind(page_number)
        .bind(hash)
        .bind(format!("{book_id}-{page_number}.png"))
        .bind("image/png")
        .bind(100_i64 + page_number)
        .execute(&pool)
        .await
        .expect("page hash match sample media page row should be inserted");
    }

    pool.close().await;
}

async fn update_book_url(paths: &RuntimeDbPaths, book_id: &str, url: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("book url update db should open");

    sqlx::query("UPDATE BOOK SET URL = ? WHERE ID = ?")
        .bind(url)
        .bind(book_id)
        .execute(&pool)
        .await
        .expect("book url should be updated");

    pool.close().await;
}

async fn update_media_page_file_size_to_null(paths: &RuntimeDbPaths, book_id: &str, number: i64) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("media page update db should open");

    sqlx::query("UPDATE MEDIA_PAGE SET FILE_SIZE = NULL WHERE BOOK_ID = ? AND NUMBER = ?")
        .bind(book_id)
        .bind(number)
        .execute(&pool)
        .await
        .expect("media page file size should be updated to null");

    pool.close().await;
}

async fn load_media_page_file_size(
    paths: &RuntimeDbPaths,
    book_id: &str,
    number: i64,
) -> Option<i64> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("media page query db should open");

    let row = sqlx::query("SELECT FILE_SIZE FROM MEDIA_PAGE WHERE BOOK_ID = ? AND NUMBER = ?")
        .bind(book_id)
        .bind(number)
        .fetch_one(&pool)
        .await
        .expect("media page row should load");

    pool.close().await;
    row.get::<Option<i64>, _>("FILE_SIZE")
}

async fn seed_kobo_sync_api_key(paths: &RuntimeDbPaths, api_key: &str, user_id: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
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

async fn seed_syncpoints(paths: &RuntimeDbPaths, rows: &[(&str, &str, Option<&str>)]) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint db should open");

    for (id, user_id, key_id) in rows {
        sqlx::query("INSERT INTO SYNC_POINT (ID, USER_ID, API_KEY_ID) VALUES (?, ?, ?)")
            .bind(id)
            .bind(user_id)
            .bind(key_id)
            .execute(&pool)
            .await
            .expect("syncpoint row should be inserted");
    }

    pool.close().await;
}

async fn seed_syncpoint_children(paths: &RuntimeDbPaths, sync_point_id: &str) {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint child db should open");

    sqlx::query(
        "INSERT INTO SYNC_POINT_BOOK (SYNC_POINT_ID, BOOK_ID, BOOK_CREATED_DATE, BOOK_LAST_MODIFIED_DATE, BOOK_FILE_LAST_MODIFIED, BOOK_FILE_SIZE, BOOK_FILE_HASH, BOOK_METADATA_LAST_MODIFIED_DATE, SYNCED) \
         VALUES (?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 1, ?, CURRENT_TIMESTAMP, 0)",
    )
    .bind(sync_point_id)
    .bind(format!("book-{sync_point_id}"))
    .bind(format!("hash-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint book row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_BOOK_REMOVED_SYNCED (SYNC_POINT_ID, BOOK_ID) VALUES (?, ?)",
    )
    .bind(sync_point_id)
    .bind(format!("removed-book-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint removed-book row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_READLIST (SYNC_POINT_ID, READLIST_ID, READLIST_NAME, READLIST_CREATED_DATE, READLIST_LAST_MODIFIED_DATE, SYNCED) \
         VALUES (?, ?, ?, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP, 0)",
    )
    .bind(sync_point_id)
    .bind(format!("readlist-{sync_point_id}"))
    .bind(format!("Readlist {sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint readlist row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_READLIST_BOOK (SYNC_POINT_ID, READLIST_ID, BOOK_ID) VALUES (?, ?, ?)",
    )
    .bind(sync_point_id)
    .bind(format!("readlist-{sync_point_id}"))
    .bind(format!("book-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint readlist book row should be inserted");

    sqlx::query(
        "INSERT INTO SYNC_POINT_READLIST_REMOVED_SYNCED (SYNC_POINT_ID, READLIST_ID) VALUES (?, ?)",
    )
    .bind(sync_point_id)
    .bind(format!("removed-readlist-{sync_point_id}"))
    .execute(&pool)
    .await
    .expect("syncpoint removed-readlist row should be inserted");

    pool.close().await;
}

async fn load_syncpoint_child_counts(paths: &RuntimeDbPaths, sync_point_id: &str) -> [i64; 5] {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint child count db should open");

    let counts = [
        load_syncpoint_child_count(&pool, "SYNC_POINT_BOOK", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_BOOK_REMOVED_SYNCED", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_READLIST", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_READLIST_BOOK", sync_point_id).await,
        load_syncpoint_child_count(&pool, "SYNC_POINT_READLIST_REMOVED_SYNCED", sync_point_id)
            .await,
    ];

    pool.close().await;
    counts
}

async fn load_syncpoint_child_count(
    pool: &sqlx::SqlitePool,
    table: &str,
    sync_point_id: &str,
) -> i64 {
    let sql = format!("SELECT COUNT(*) AS COUNT FROM {table} WHERE SYNC_POINT_ID = ?");
    sqlx::query(&sql)
        .bind(sync_point_id)
        .fetch_one(pool)
        .await
        .expect("syncpoint child count should load")
        .get::<i64, _>("COUNT")
}

async fn load_syncpoint_ids(paths: &RuntimeDbPaths) -> Vec<String> {
    let pool = connect_pool(paths.main_db.as_path(), 1)
        .await
        .expect("syncpoint query db should open");

    let rows = sqlx::query("SELECT ID FROM SYNC_POINT ORDER BY ID")
        .fetch_all(&pool)
        .await
        .expect("syncpoint rows should load");
    pool.close().await;

    rows.into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect()
}
