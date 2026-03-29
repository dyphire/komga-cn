use super::*;
use crate::tasks::{
    delete_book_rows, delete_series_rows, load_book_delete_decision, load_book_delete_work,
    load_series_delete_work,
};

pub(super) fn delete_book_task(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<(), TaskExecutionError> {
    let target = load_book_delete_target(runtime, book_id)?;
    let Some((series_id, oneshot)) = target else {
        return Ok(());
    };

    if oneshot {
        delete_series(runtime, &series_id)
    } else {
        delete_book(runtime, book_id)
    }
}

fn load_book_delete_target(
    runtime: &RuntimeConfig,
    book_id: &str,
) -> Result<Option<(String, bool)>, TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    Ok(
        load_book_delete_decision(runtime.database_file.as_path(), book_id)
            .map_err(TaskExecutionError::runtime)?
            .map(|target| (target.series_id, target.oneshot)),
    )
}

fn delete_book(runtime: &RuntimeConfig, book_id: &str) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let Some(work) = load_book_delete_work(runtime.database_file.as_path(), book_id)
        .map_err(TaskExecutionError::runtime)?
    else {
        return Ok(());
    };

    let _ = fs::remove_file(&work.book_path);

    delete_book_rows(runtime.database_file.as_path(), book_id, &work.series_id)
        .map_err(TaskExecutionError::runtime)
}

pub(super) fn delete_series(
    runtime: &RuntimeConfig,
    series_id: &str,
) -> Result<(), TaskExecutionError> {
    let runtime = runtime.task_runtime_context();
    let work = load_series_delete_work(runtime.database_file.as_path(), series_id)
        .map_err(TaskExecutionError::runtime)?;

    for book_path in &work.book_paths {
        let _ = fs::remove_file(book_path);
    }

    delete_series_rows(runtime.database_file.as_path(), series_id, &work.book_ids)
        .map_err(TaskExecutionError::runtime)
}
