use super::*;

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
            let series_id =
                match super::super::metadata_tasks::refresh_book_metadata(runtime, book_id) {
                    Ok(series_id) => series_id,
                    Err(error) => return Some(Err(error)),
                };
            if let Some(series_id) = series_id {
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("REFRESH_SERIES_METADATA:{series_id}"),
                    task.priority.saturating_sub(5),
                    Some(series_id),
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
            scheduler.enqueue(TaskQueueRecord::new(
                format!("AGGREGATE_SERIES_METADATA:{series_id}"),
                task.priority.saturating_sub(5),
                Some(series_id.to_string()),
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
            super::super::metadata_tasks::refresh_book_local_artwork(runtime, book_id)
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
