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
                                ))))
                            }
                        }
                    }
                    Err(error) => {
                        return Some(Err(TaskExecutionError::runtime(format!(
                            "build runtime for thumbnail finder settings failed: {error}"
                        ))))
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
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("GENERATE_BOOK_THUMBNAIL:{book_id}"),
                    task.priority.saturating_sub(5),
                    Some(book_id),
                ));
            }
            Ok(())
        }
        _ => return None,
    };

    Some(result)
}
