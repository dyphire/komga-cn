use super::{RuntimeConfig, TaskExecutionError, TaskQueueRecord, TaskQueueScheduler};
use crate::operational_settings_access::load_server_settings;
use crate::sqlite::write_models::ServerSettingsStore;
use serde_json::Value;

fn thumbnail_max_edge(thumbnail_size: &str) -> i64 {
    match thumbnail_size {
        "MEDIUM" => 600,
        "LARGE" => 900,
        "XLARGE" => 1200,
        _ => 300,
    }
}

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let result = match task.simple_type.as_str() {
        "ANALYZE_BOOK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "ANALYZE_BOOK task must include a book id",
                )));
            };
            super::index_tasks::analyze_book(runtime, book_id)
        }
        "REBUILD_INDEX" => super::index_tasks::rebuild_index(runtime),
        "FIND_BOOK_THUMBNAILS_TO_REGENERATE" => {
            let payload = task
                .payload
                .as_deref()
                .and_then(|payload| serde_json::from_str::<Value>(payload).ok());
            let for_bigger_result_only = payload
                .as_ref()
                .and_then(|payload| payload.get("for_bigger_result_only"))
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let book_ids = if for_bigger_result_only {
                let runtime_context = runtime.task_runtime_context();
                let settings_store =
                    ServerSettingsStore::new(runtime_context.database_file.clone());
                let settings = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(async_runtime) => {
                        match async_runtime.block_on(load_server_settings(&settings_store)) {
                            Ok(settings) => settings,
                            Err(error) => {
                                return Some(Err(TaskExecutionError::runtime(format!(
                                    "load server settings for thumbnail finder failed: {error}"
                                ))));
                            }
                        }
                    }
                    Err(error) => {
                        return Some(Err(TaskExecutionError::runtime(format!(
                            "build runtime for thumbnail finder settings failed: {error}"
                        ))));
                    }
                };
                let max_edge = thumbnail_max_edge(settings.thumbnail_size);
                match super::find_books_with_undersized_generated_thumbnails(runtime, max_edge) {
                    Ok(ids) => ids,
                    Err(error) => return Some(Err(error)),
                }
            } else {
                match super::find_books_without_selected_thumbnails(runtime) {
                    Ok(ids) => ids,
                    Err(error) => return Some(Err(error)),
                }
            };
            for book_id in book_ids {
                scheduler.enqueue(
                    TaskQueueRecord::new(
                        format!("GENERATE_BOOK_THUMBNAIL_{book_id}"),
                        task.priority.saturating_sub(5),
                        Some(book_id),
                    )
                    .with_simple_type("GENERATE_BOOK_THUMBNAIL"),
                );
            }
            Ok(())
        }
        _ => return None,
    };

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sqlite::connect_pool;
    use komga_application::task_processing::TaskQueueAdminPort;
    use komga_application::task_processing::TaskRuntimeContext;

    fn unique_temp_path(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock should be after unix epoch")
                .as_nanos()
        ))
    }

    #[tokio::test]
    async fn thumbnail_finder_enqueues_kotlin_style_generate_thumbnail_ids() {
        let database_file = unique_temp_path("komga-thumbnail-finder-main");
        let tasks_db_file = unique_temp_path("komga-thumbnail-finder-tasks");
        let lucene_dir = unique_temp_path("komga-thumbnail-finder-lucene");

        let pool = connect_pool(database_file.as_path(), 1)
            .await
            .expect("thumbnail finder test db should open");
        sqlx::query(
            "CREATE TABLE BOOK (ID varchar NOT NULL PRIMARY KEY, DELETED_DATE timestamp NULL)",
        )
        .execute(&pool)
        .await
        .expect("book table should be created");
        sqlx::query(
            "CREATE TABLE THUMBNAIL_BOOK (ID varchar NOT NULL PRIMARY KEY, BOOK_ID varchar NOT NULL, SELECTED integer NOT NULL)",
        )
        .execute(&pool)
        .await
        .expect("thumbnail book table should be created");
        sqlx::query("INSERT INTO BOOK (ID, DELETED_DATE) VALUES (?, NULL)")
            .bind("book-1")
            .execute(&pool)
            .await
            .expect("book row should be inserted");
        pool.close().await;

        let runtime = TaskRuntimeContext {
            database_file: database_file.clone(),
            tasks_db_file,
            lucene_data_directory: lucene_dir,
            consumes_queue: false,
            owns_main_database: true,
            owns_filesystem_scan_output: true,
            owns_sidecar_output: true,
            owns_search_index: true,
        };
        let mut scheduler =
            TaskQueueScheduler::for_runtime(runtime.clone(), "thumbnail-finder-test");
        let finder_task = TaskQueueRecord::new("FIND_BOOK_THUMBNAILS_TO_REGENERATE", 0, None)
            .with_payload(serde_json::json!({ "for_bigger_result_only": false }).to_string());

        let result = try_execute(&mut scheduler, &runtime, &finder_task, None);
        assert!(matches!(result, Some(Ok(()))));

        let generated = scheduler
            .admin_mut()
            .take_available("thumbnail-finder-assert")
            .expect("finder should enqueue one generate thumbnail task");

        assert_eq!(generated.id, "GENERATE_BOOK_THUMBNAIL_book-1");
        assert_eq!(generated.simple_type, "GENERATE_BOOK_THUMBNAIL");
        assert_eq!(generated.group, Some("book-1".to_string()));

        let _ = std::fs::remove_file(database_file);
    }
}
