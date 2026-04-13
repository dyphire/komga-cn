use super::*;
use std::collections::BTreeSet;

use serde_json::Value;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    let result = match task.simple_type.as_str() {
        "REFRESH_BOOK_METADATA" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REFRESH_BOOK_METADATA task must include a book id",
                )));
            };
            let capabilities = refresh_book_metadata_capabilities(task);
            let series_id = match super::super::metadata_tasks::refresh_book_metadata(
                runtime,
                book_id,
                &capabilities,
            ) {
                Ok(series_id) => series_id,
                Err(error) => return Some(Err(error)),
            };
            if let Some(series_id) = series_id {
                scheduler.enqueue(runtime_follow_up_task(
                    RuntimeFollowUpTask::RefreshSeriesMetadata {
                        series_id,
                        priority: task.priority.saturating_sub(5),
                    },
                ));
            }
            Ok(())
        }
        "REFRESH_SERIES_METADATA" => {
            let Some(series_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REFRESH_SERIES_METADATA task must include a series id",
                )));
            };
            if let Err(error) =
                super::super::metadata_tasks::refresh_series_metadata(runtime, series_id)
            {
                return Some(Err(error));
            }
            scheduler.enqueue(runtime_follow_up_task(
                RuntimeFollowUpTask::AggregateSeriesMetadata {
                    series_id: series_id.to_string(),
                    priority: task.priority.saturating_sub(5),
                },
            ));
            Ok(())
        }
        "AGGREGATE_SERIES_METADATA" => {
            let Some(series_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "AGGREGATE_SERIES_METADATA task must include a series id",
                )));
            };
            super::super::metadata_tasks::aggregate_series_metadata(runtime, series_id)
        }
        "REFRESH_BOOK_LOCAL_ARTWORK" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REFRESH_BOOK_LOCAL_ARTWORK task must include a book id",
                )));
            };
            super::super::metadata_tasks::refresh_book_local_artwork(runtime, book_id)
        }
        "GENERATE_BOOK_THUMBNAIL" => {
            let Some(book_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "GENERATE_BOOK_THUMBNAIL task must include a book id",
                )));
            };
            super::super::metadata_tasks::generate_book_thumbnail(runtime, book_id)
        }
        "REFRESH_SERIES_LOCAL_ARTWORK" => {
            let Some(series_id) = task_target else {
                return Some(Err(TaskExecutionError::invalid_task(
                    "REFRESH_SERIES_LOCAL_ARTWORK task must include a series id",
                )));
            };
            super::super::metadata_tasks::refresh_series_local_artwork(runtime, series_id)
        }
        _ => return None,
    };

    Some(result)
}

fn refresh_book_metadata_capabilities(task: &TaskQueueRecord) -> BTreeSet<String> {
    task.payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        .and_then(|payload| payload.get("capabilities").cloned())
        .and_then(|capabilities| capabilities.as_array().cloned())
        .map(|capabilities| {
            capabilities
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<BTreeSet<_>>()
        })
        .filter(|capabilities| !capabilities.is_empty())
        .unwrap_or_else(default_refresh_book_metadata_capabilities)
}

fn default_refresh_book_metadata_capabilities() -> BTreeSet<String> {
    [
        "TITLE",
        "SUMMARY",
        "NUMBER",
        "NUMBER_SORT",
        "RELEASE_DATE",
        "AUTHORS",
        "TAGS",
        "ISBN",
        "READ_LISTS",
        "THUMBNAILS",
        "LINKS",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
