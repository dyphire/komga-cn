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
    let library_id = task
        .payload
        .as_deref()
        .and_then(parse_scan_library_payload_library_id)
        .or_else(|| task_target.map(strip_scan_library_deep_suffix));
    let Some(library_id) = library_id else {
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
    let renumbered_book_ids =
        persist_scanned_library(runtime_context.database_file.as_path(), &library_id, &scan)
            .map_err(TaskExecutionError::runtime)?;

    if library_empty_trash_after_scan(runtime_context.database_file.as_path(), &library_id)
        .map_err(TaskExecutionError::runtime)?
    {
        super::super::cleanup_tasks::empty_trash(runtime, &library_id)?;
    }
    super::super::cleanup_tasks::cleanup_empty_sets(runtime)?;

    const DEFAULT_PRIORITY: i32 = 4;
    const LOW_PRIORITY: i32 = 2;
    const LOWEST_PRIORITY: i32 = 0;

    let hashing_flags = load_library_hashing_flags(runtime, &library_id)?;
    let analyzable_book_ids = find_books_requiring_analysis(runtime, &scan.book_ids)?
        .into_iter()
        .collect::<HashSet<_>>();
    for series in &scan.series_rows {
        for book in &series.books {
            if analyzable_book_ids.contains(&book.book_id) {
                scheduler.enqueue(TaskQueueRecord::new(
                    format!("ANALYZE_BOOK:{}", book.book_id),
                    DEFAULT_PRIORITY,
                    Some(series.series_id.clone()),
                ));
            }
        }
    }

    if hashing_flags.hash_files {
        let book_ids = find_books_with_missing_file_hash(runtime, &library_id, false)?;
        for book_id in book_ids {
            scheduler.enqueue(hash_book_task(&book_id, 0));
        }
    }

    if hashing_flags.hash_koreader {
        let book_ids = find_books_with_missing_file_hash(runtime, &library_id, true)?;
        for book_id in book_ids {
            scheduler.enqueue(hash_book_koreader_task(&book_id, 0));
        }
    }

    if hashing_flags.hash_pages {
        // Kotlin deliberately leaves the library-wide page-hash sweep at lowest priority so scan
        // completion can unblock analyze/metadata follow-ups before the longer hash backlog runs.
        scheduler.enqueue(find_books_with_missing_page_hash_task(&library_id));
    }
    scheduler.enqueue(find_duplicate_pages_to_delete_task(
        &library_id,
        LOWEST_PRIORITY,
    ));

    let maintenance_flags = load_library_maintenance_flags(runtime, &library_id)?;
    if maintenance_flags.repair_extensions {
        let books = find_books_for_extension_repair(runtime, &library_id)?;
        for book in books {
            scheduler.enqueue(repair_extension_task(
                &book.book_id,
                &book.series_id,
                LOW_PRIORITY,
            ));
        }
    }
    if maintenance_flags.convert_to_cbz {
        scheduler.enqueue(TaskQueueRecord::new(
            format!("FIND_BOOKS_TO_CONVERT:{library_id}"),
            LOWEST_PRIORITY,
            None,
        ));
    }

    let book_series_ids = scan
        .series_rows
        .iter()
        .flat_map(|series| {
            series
                .books
                .iter()
                .map(|book| (book.book_id.clone(), series.series_id.clone()))
        })
        .collect::<HashMap<_, _>>();
    for book_id in renumbered_book_ids {
        scheduler.enqueue(
            TaskQueueRecord::new(
                format!("REFRESH_BOOK_METADATA_{book_id}"),
                DEFAULT_PRIORITY,
                book_series_ids.get(&book_id).cloned(),
            )
            .with_simple_type("REFRESH_BOOK_METADATA"),
        );
    }

    enqueue_sidecar_refresh_tasks(scheduler, &scan, &changed_sidecars, DEFAULT_PRIORITY);
    Ok(())
}

fn parse_scan_library_payload_library_id(payload: &str) -> Option<String> {
    let payload = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    payload.get("libraryId")?.as_str().map(str::to_string)
}

fn strip_scan_library_deep_suffix(task_target: &str) -> String {
    task_target
        .split_once(":DEEP:")
        .map(|(id, _)| id)
        .unwrap_or(task_target)
        .to_string()
}

fn find_duplicate_pages_to_delete_task(library_id: &str, priority: i32) -> TaskQueueRecord {
    TaskQueueRecord::new(
        format!("FIND_DUPLICATE_PAGES_TO_DELETE_{library_id}"),
        priority,
        None,
    )
    .with_simple_type("FIND_DUPLICATE_PAGES_TO_DELETE")
}

fn hash_book_task(book_id: &str, priority: i32) -> TaskQueueRecord {
    let task_id = format!("HASH_BOOK_{book_id}");
    TaskQueueRecord::new(task_id.clone(), priority, None)
        .with_simple_type("HASH_BOOK")
        .with_payload(
            serde_json::json!({
                "bookId": book_id,
                "priority": priority,
                "groupId": serde_json::Value::Null,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

fn hash_book_koreader_task(book_id: &str, priority: i32) -> TaskQueueRecord {
    let task_id = format!("HASH_BOOK_KOREADER_{book_id}");
    TaskQueueRecord::new(task_id.clone(), priority, None)
        .with_simple_type("HASH_BOOK_KOREADER")
        .with_payload(
            serde_json::json!({
                "bookId": book_id,
                "priority": priority,
                "groupId": serde_json::Value::Null,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

fn find_books_with_missing_page_hash_task(library_id: &str) -> TaskQueueRecord {
    let task_id = format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH_{library_id}");
    TaskQueueRecord::new(task_id.clone(), 0, None)
        .with_simple_type("FIND_BOOKS_WITH_MISSING_PAGE_HASH")
        .with_payload(
            serde_json::json!({
                "libraryId": library_id,
                "priority": 0,
                "groupId": serde_json::Value::Null,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

fn repair_extension_task(book_id: &str, series_id: &str, priority: i32) -> TaskQueueRecord {
    let task_id = format!("REPAIR_EXTENSION_{book_id}");
    TaskQueueRecord::new(task_id.clone(), priority, Some(series_id.to_string()))
        .with_simple_type("REPAIR_EXTENSION")
        .with_payload(
            serde_json::json!({
                "bookId": book_id,
                "priority": priority,
                "groupId": series_id,
                "uniqueId": task_id,
            })
            .to_string(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_duplicate_pages_to_delete_task_uses_kotlin_compatible_unique_id() {
        let task = find_duplicate_pages_to_delete_task("library-1", 85);

        assert_eq!(
            task.id,
            "FIND_DUPLICATE_PAGES_TO_DELETE_library-1".to_string()
        );
        assert_eq!(
            task.simple_type,
            "FIND_DUPLICATE_PAGES_TO_DELETE".to_string()
        );
        assert_eq!(task.priority, 85);
        assert_eq!(task.group, None);
        assert!(task.payload.is_none());
    }

    #[test]
    fn hash_book_task_uses_kotlin_compatible_unique_id() {
        let task = hash_book_task("book-1", 0);

        assert_eq!(task.id, "HASH_BOOK_book-1".to_string());
        assert_eq!(task.simple_type, "HASH_BOOK".to_string());
        assert_eq!(task.priority, 0);
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":null,"priority":0,"uniqueId":"HASH_BOOK_book-1"}"#
            ),
        );
    }

    #[test]
    fn hash_book_koreader_task_uses_kotlin_compatible_unique_id() {
        let task = hash_book_koreader_task("book-1", 0);

        assert_eq!(task.id, "HASH_BOOK_KOREADER_book-1".to_string());
        assert_eq!(task.simple_type, "HASH_BOOK_KOREADER".to_string());
        assert_eq!(task.priority, 0);
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":null,"priority":0,"uniqueId":"HASH_BOOK_KOREADER_book-1"}"#
            ),
        );
    }

    #[test]
    fn find_books_with_missing_page_hash_task_uses_kotlin_compatible_unique_id() {
        let task = find_books_with_missing_page_hash_task("library-1");

        assert_eq!(
            task.id,
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1".to_string()
        );
        assert_eq!(
            task.simple_type,
            "FIND_BOOKS_WITH_MISSING_PAGE_HASH".to_string()
        );
        assert_eq!(task.group, None);
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"groupId":null,"libraryId":"library-1","priority":0,"uniqueId":"FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-1"}"#
            ),
        );
    }

    #[test]
    fn repair_extension_task_uses_kotlin_compatible_unique_id() {
        let task = repair_extension_task("book-1", "series-1", 12);

        assert_eq!(task.id, "REPAIR_EXTENSION_book-1".to_string());
        assert_eq!(task.simple_type, "REPAIR_EXTENSION".to_string());
        assert_eq!(task.priority, 12);
        assert_eq!(task.group.as_deref(), Some("series-1"));
        assert_eq!(
            task.payload.as_deref(),
            Some(
                r#"{"bookId":"book-1","groupId":"series-1","priority":12,"uniqueId":"REPAIR_EXTENSION_book-1"}"#
            ),
        );
    }
}
