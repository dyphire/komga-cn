use super::*;

pub(super) fn try_execute(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Option<Result<(), TaskExecutionError>> {
    if task.simple_type != "SCAN_LIBRARY" {
        return None;
    }

    Some(handle_scan_library(scheduler, runtime, task, task_target))
}

fn handle_scan_library(
    scheduler: &mut TaskQueueScheduler,
    runtime: &RuntimeConfig,
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<(), TaskExecutionError> {
    let Some(library_id) = task_target else {
        return Err(TaskExecutionError::invalid_task(
            "SCAN_LIBRARY task must include a library id",
        ));
    };
    let library_id = library_id.to_string();

    let deep_scan = task
        .payload
        .as_deref()
        .and_then(parse_scan_library_payload_deep)
        .unwrap_or(false);
    let runtime_context = runtime.task_runtime_context();
    if !runtime_context.owns_filesystem_scan_output {
        return Ok(());
    }

    let scan = scan_library(
        runtime_context.database_file.as_path(),
        &library_id,
        deep_scan,
    )
    .map_err(TaskExecutionError::runtime)?;
    let changed_sidecars = load_changed_sidecars(
        runtime_context.database_file.as_path(),
        &library_id,
        &scan.sidecars,
    )
    .map_err(TaskExecutionError::runtime)?;
    persist_scanned_library(runtime_context.database_file.as_path(), &library_id, &scan)
        .map_err(TaskExecutionError::runtime)?;

    if library_empty_trash_after_scan(runtime_context.database_file.as_path(), &library_id)
        .map_err(TaskExecutionError::runtime)?
    {
        super::super::cleanup_tasks::empty_trash(runtime, &library_id)?;
    }
    super::super::cleanup_tasks::cleanup_empty_sets(runtime)?;

    let hashing_flags = load_library_hashing_flags(runtime, &library_id)?;
    let analyzable_book_ids = find_books_requiring_analysis(runtime, &scan.book_ids)?;
    for book_id in &analyzable_book_ids {
        scheduler.enqueue(TaskQueueRecord::new(
            format!("ANALYZE_BOOK:{book_id}"),
            task.priority.saturating_sub(10),
            Some(book_id.clone()),
        ));
    }

    if hashing_flags.hash_files {
        let book_ids = find_books_with_missing_file_hash(runtime, &library_id, false)?;
        for book_id in book_ids {
            scheduler.enqueue(TaskQueueRecord::new(
                format!("HASH_BOOK:{book_id}"),
                task.priority.saturating_sub(15),
                Some(book_id),
            ));
        }
    }

    if hashing_flags.hash_koreader {
        let book_ids = find_books_with_missing_file_hash(runtime, &library_id, true)?;
        for book_id in book_ids {
            scheduler.enqueue(TaskQueueRecord::new(
                format!("HASH_BOOK_KOREADER:{book_id}"),
                task.priority.saturating_sub(15),
                Some(book_id),
            ));
        }
    }

    if hashing_flags.hash_pages {
        scheduler.enqueue(TaskQueueRecord::new(
            format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH:{library_id}"),
            task.priority.saturating_sub(15),
            Some(library_id.clone()),
        ));
    }
    scheduler.enqueue(TaskQueueRecord::new(
        format!("FIND_DUPLICATE_PAGES_TO_DELETE:{library_id}"),
        task.priority.saturating_sub(15),
        Some(library_id.clone()),
    ));

    let maintenance_flags = load_library_maintenance_flags(runtime, &library_id)?;
    if maintenance_flags.repair_extensions {
        scheduler.enqueue(TaskQueueRecord::new(
            format!("REPAIR_EXTENSIONS:{library_id}"),
            task.priority.saturating_sub(20),
            Some(library_id.clone()),
        ));
    }
    if maintenance_flags.convert_to_cbz {
        scheduler.enqueue(TaskQueueRecord::new(
            format!("FIND_BOOKS_TO_CONVERT:{library_id}"),
            task.priority.saturating_sub(20),
            Some(library_id.clone()),
        ));
    }

    enqueue_sidecar_refresh_tasks(
        scheduler,
        &scan,
        &changed_sidecars,
        task.priority.saturating_sub(12),
    );
    Ok(())
}
