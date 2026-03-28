use std::hash::{Hash, Hasher};
use std::io;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use axum::Router;
use komga_persistence::sqlite::connect_pool;
use serde_json::json;
use sqlx::Row;
use tokio::net::TcpListener;

use crate::config::RuntimeConfig;
use crate::scanner::{ScanResult, scan_root_folder};

pub use komga_server::app::CompatProfile;

pub fn build_router() -> Router {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    build_router_with_config(&config)
}

pub fn build_router_with_profile(profile: CompatProfile) -> Router {
    komga_server::app::build_router_with_profile(profile)
}

pub fn build_router_with_config(config: &RuntimeConfig) -> Router {
    persist_scanner_rows(config);
    komga_server::app::build_router_with_config(config)
}

pub async fn serve(listener: TcpListener) -> io::Result<()> {
    let config = RuntimeConfig::from_env().expect("invalid runtime config");
    serve_with_config(listener, config).await
}

pub async fn serve_with_config(listener: TcpListener, config: RuntimeConfig) -> io::Result<()> {
    persist_scanner_rows(&config);
    komga_server::app::serve_with_config(listener, config).await
}

fn persist_scanner_rows(config: &RuntimeConfig) {
    let runtime_config = config.clone();
    let database_file = config.database_file.clone();
    let tasks_db_file = config.tasks_db_file.clone();

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("scanner persistence runtime should build");

        runtime.block_on(async move {
            let pool = connect_pool(&database_file, 1)
                .await
                .expect("main sqlite pool should open for scanner persistence");
            let tasks_pool = if tasks_db_file.exists() {
                Some(
                    connect_pool(&tasks_db_file, 1)
                        .await
                        .expect("tasks sqlite pool should open for scanner task persistence"),
                )
            } else {
                None
            };

            let libraries = sqlx::query(
                "SELECT ID, ROOT \
                                         FROM LIBRARY",
            )
            .fetch_all(&pool)
            .await
            .expect("library rows should be queryable for scanner persistence");

            for library in libraries {
                let library_id = library.get::<String, _>("ID");
                let root = library.get::<String, _>("ROOT");
                let scan_result =
                    match scan_root_folder(PathBuf::from(&root).as_path(), &Default::default()) {
                        Ok(result) => result,
                        Err(_) => continue,
                    };

                persist_library_scan(&pool, &library_id, &scan_result)
                    .await
                    .expect("scan rows should persist into Kotlin-compatible tables");

                if let Some(tasks_pool) = &tasks_pool {
                    persist_scanner_tasks(tasks_pool, &library_id, &scan_result)
                        .await
                        .expect(
                            "scanner task rows should persist into Kotlin-compatible TASK table",
                        );
                }
            }

            pool.close().await;
            if let Some(tasks_pool) = tasks_pool {
                tasks_pool.close().await;
            }

            let mut scheduler = crate::task_queue::TaskQueueScheduler::for_runtime(
                runtime_config.clone(),
                "rust-main",
            );
            let _ = scheduler.process_available(&runtime_config);
        })
    })
    .join()
    .expect("scanner persistence worker thread should complete");
}

async fn persist_library_scan(
    pool: &sqlx::SqlitePool,
    library_id: &str,
    scan_result: &ScanResult,
) -> Result<(), sqlx::Error> {
    for scanned_series in &scan_result.series {
        let series = &scanned_series.series;
        let series_id = route_safe_scanner_id("series", &series.path);
        let series_url = series.path.to_string_lossy().to_string();

        sqlx::query(
            "INSERT \
             OR IGNORE INTO SERIES (ID, FILE_LAST_MODIFIED, NAME, URL, LIBRARY_ID, oneshot) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&series_id)
        .bind(to_unix_seconds(series.file_last_modified))
        .bind(&series.name)
        .bind(series_url)
        .bind(library_id)
        .bind(series.oneshot)
        .execute(pool)
        .await?;

        for book in &scanned_series.books {
            let book_id = route_safe_scanner_id("book", &book.path);
            let book_url = book.path.to_string_lossy().to_string();
            let book_file_name = book
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_string();

            sqlx::query(
                "INSERT \
                 OR IGNORE INTO BOOK (ID, FILE_LAST_MODIFIED, NAME, URL, SERIES_ID, FILE_SIZE, \
                   LIBRARY_ID, oneshot) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&book_id)
            .bind(to_unix_seconds(book.file_last_modified))
            .bind(&book_file_name)
            .bind(book_url)
            .bind(&series_id)
            .bind(book.file_size as i64)
            .bind(library_id)
            .bind(book.oneshot)
            .execute(pool)
            .await?;

            sqlx::query(
                "INSERT INTO MEDIA_FILE (FILE_NAME, BOOK_ID, FILE_SIZE) SELECT ?, ?, ? \
                 WHERE NOT EXISTS (SELECT 1 \
                 FROM MEDIA_FILE \
                 WHERE FILE_NAME = ? \
                 AND BOOK_ID = ?)",
            )
            .bind(&book_file_name)
            .bind(&book_id)
            .bind(book.file_size as i64)
            .bind(&book_file_name)
            .bind(&book_id)
            .execute(pool)
            .await?;
        }
    }

    for sidecar in &scan_result.sidecars {
        sqlx::query(
            "INSERT \
             OR IGNORE INTO SIDECAR (URL, PARENT_URL, LAST_MODIFIED_TIME, LIBRARY_ID) \
             VALUES (?, ?, ?, ?)",
        )
        .bind(sidecar.path.to_string_lossy().to_string())
        .bind(sidecar.target_path.to_string_lossy().to_string())
        .bind(to_unix_seconds(sidecar.file_last_modified))
        .bind(library_id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn persist_scanner_tasks(
    tasks_pool: &sqlx::SqlitePool,
    library_id: &str,
    scan_result: &ScanResult,
) -> Result<(), sqlx::Error> {
    if scan_result.series.is_empty() {
        return Ok(());
    }

    persist_task_row(
        tasks_pool,
        &format!("SCAN_LIBRARY:{library_id}"),
        100,
        Some(library_id),
        "SCAN_LIBRARY",
    )
    .await?;

    for scanned_series in &scan_result.series {
        for book in &scanned_series.books {
            let book_id = route_safe_scanner_id("book", &book.path);
            persist_task_row(
                tasks_pool,
                &format!("ANALYZE_BOOK:{book_id}"),
                90,
                Some(&book_id),
                "ANALYZE_BOOK",
            )
            .await?;
        }
    }

    Ok(())
}

fn route_safe_scanner_id(prefix: &str, path: &std::path::Path) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy().hash(&mut hasher);
    format!("{prefix}-{:016x}", hasher.finish())
}

async fn persist_task_row(
    tasks_pool: &sqlx::SqlitePool,
    id: &str,
    priority: i32,
    group_id: Option<&str>,
    simple_type: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO TASK (ID, PRIORITY, GROUP_ID, CLASS, SIMPLE_TYPE, PAYLOAD, OWNER) \
         VALUES (?, ?, ?, ?, ?, ?, NULL) \
         ON CONFLICT(ID) DO NOTHING",
    )
    .bind(id)
    .bind(priority)
    .bind(group_id)
    .bind(kotlin_task_class_name(simple_type))
    .bind(simple_type)
    .bind(task_payload(id, priority, group_id, simple_type))
    .execute(tasks_pool)
    .await?;

    Ok(())
}

fn kotlin_task_class_name(simple_type: &str) -> String {
    format!(
        "org.gotson.komga.task.{}.CompatTask",
        simple_type.to_ascii_lowercase(),
    )
}

fn task_payload(id: &str, priority: i32, group_id: Option<&str>, simple_type: &str) -> String {
    json!({
        "id": id,
        "simpleType": simple_type,
        "priority": priority,
        "groupId": group_id,
    })
    .to_string()
}

fn to_unix_seconds(time: std::time::SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
