use super::*;
use komga_application::task_processing::TaskRuntimeContext;
use std::collections::BTreeSet;

use serde_json::Value;

pub(super) async fn try_execute(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<TaskExecutionOutcome, TaskExecutionError>> {
    match task.simple_type.as_str() {
        "REFRESH_BOOK_METADATA" => {
            Some(execute_refresh_book_metadata(runtime, task, task_target).await)
        }
        "REFRESH_SERIES_METADATA" => {
            Some(execute_refresh_series_metadata(runtime, task, task_target).await)
        }
        "AGGREGATE_SERIES_METADATA" => {
            Some(execute_aggregate_series_metadata(runtime, task_target).await)
        }
        "REFRESH_BOOK_LOCAL_ARTWORK" => {
            Some(execute_refresh_book_local_artwork(runtime, task_target).await)
        }
        "GENERATE_BOOK_THUMBNAIL" => {
            Some(execute_generate_book_thumbnail(runtime, task_target).await)
        }
        "REFRESH_SERIES_LOCAL_ARTWORK" => {
            Some(execute_refresh_series_local_artwork(runtime, task_target).await)
        }
        _ => None,
    }
}

async fn execute_refresh_book_metadata(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(book_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "REFRESH_BOOK_METADATA task must include a book id",
        ));
    };
    let capabilities = refresh_book_metadata_capabilities(task);
    let series_id =
        super::super::metadata_tasks::refresh_book_metadata(runtime, book_id, &capabilities)
            .await?;
    let follow_up_tasks = series_id
        .into_iter()
        .map(|series_id| {
            runtime_follow_up_task(RuntimeFollowUpTask::RefreshSeriesMetadata {
                series_id,
                priority: task.priority - 1,
            })
        })
        .collect();
    Ok(TaskExecutionOutcome::with_follow_up_tasks(follow_up_tasks))
}

async fn execute_refresh_series_metadata(
    runtime: &TaskRuntimeContext,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(series_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "REFRESH_SERIES_METADATA task must include a series id",
        ));
    };
    super::super::metadata_tasks::refresh_series_metadata(runtime, series_id).await?;
    Ok(TaskExecutionOutcome::with_follow_up_tasks(vec![
        runtime_follow_up_task(RuntimeFollowUpTask::AggregateSeriesMetadata {
            series_id: series_id.to_string(),
            priority: task.priority,
        }),
    ]))
}

async fn execute_aggregate_series_metadata(
    runtime: &TaskRuntimeContext,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(series_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "AGGREGATE_SERIES_METADATA task must include a series id",
        ));
    };
    super::super::metadata_tasks::aggregate_series_metadata(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

async fn execute_refresh_book_local_artwork(
    runtime: &TaskRuntimeContext,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(book_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "REFRESH_BOOK_LOCAL_ARTWORK task must include a book id",
        ));
    };
    super::super::metadata_tasks::refresh_book_local_artwork(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

async fn execute_generate_book_thumbnail(
    runtime: &TaskRuntimeContext,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(book_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "GENERATE_BOOK_THUMBNAIL task must include a book id",
        ));
    };
    super::super::metadata_tasks::generate_book_thumbnail(runtime, book_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
}

async fn execute_refresh_series_local_artwork(
    runtime: &TaskRuntimeContext,
    task_target: Option<&str>,
) -> Result<TaskExecutionOutcome, TaskExecutionError> {
    let Some(series_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "REFRESH_SERIES_LOCAL_ARTWORK task must include a series id",
        ));
    };
    super::super::metadata_tasks::refresh_series_local_artwork(runtime, series_id)
        .await
        .map(|()| TaskExecutionOutcome::completed())
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
