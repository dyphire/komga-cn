use super::*;
use komga_application::media_assets::{
    BooksImportEntry as ApplicationBooksImportEntry,
    BooksImportPayload as ApplicationBooksImportPayload,
    ImportCopyMode as ApplicationImportCopyMode,
};
use komga_application::task_processing::TaskQueueRecord as ApplicationTaskQueueRecord;
use tracing::error;

use crate::media_assets_runtime_access::RuntimeMediaImportService;

pub async fn books_import(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let payload = match parse_books_import_payload(&body) {
        Ok(payload) => payload,
        Err(error) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": error }))).into_response();
        }
    };

    let service = media_import_service(auth_db.database_file.as_path());
    enqueue_books_best_effort(service.as_ref(), payload, |task_record| {
        process_task_side_effects(&state, vec![task_record])
    });

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_runtime_owned(&mut response);
    response
}

fn enqueue_books_best_effort(
    service: &dyn RuntimeMediaImportService,
    payload: BooksImportPayload,
    mut enqueue_task: impl FnMut(TaskQueueRecord) -> Result<(), String>,
) {
    for book in payload.books {
        let source_file = book.source_file.display().to_string();
        let series_id = book.series_id.clone();
        let task_id = kotlin_import_book_task_id(&book);
        match service
            .enqueue_books(
                application_import_payload(BooksImportPayload {
                    copy_mode: payload.copy_mode,
                    books: vec![book],
                }),
                &mut || task_id.clone(),
            )
            .and_then(|mut task_records| {
                task_records
                    .pop()
                    .ok_or_else(|| "import task generation returned no task".to_string())
            })
        {
            Ok(task_record) => {
                if let Err(err) = enqueue_task(interface_task_record(task_record)) {
                    error!(
                        %series_id,
                        source_file = %source_file,
                        error = %err,
                        "Failed to enqueue import task"
                    );
                }
            }
            Err(err) => {
                error!(
                    %series_id,
                    source_file = %source_file,
                    error = %err,
                    "Failed to create import task"
                );
            }
        }
    }
}

fn kotlin_import_book_task_id(book: &BooksImportEntry) -> String {
    format!("IMPORT_BOOK_{}_{}", book.series_id, book.source_file.display())
}

fn application_import_payload(payload: BooksImportPayload) -> ApplicationBooksImportPayload {
    ApplicationBooksImportPayload {
        copy_mode: match payload.copy_mode {
            ImportCopyMode::Move => ApplicationImportCopyMode::Move,
            ImportCopyMode::Copy => ApplicationImportCopyMode::Copy,
            ImportCopyMode::Hardlink => ApplicationImportCopyMode::Hardlink,
        },
        books: payload
            .books
            .into_iter()
            .map(|book| ApplicationBooksImportEntry {
                source_file: book.source_file,
                series_id: book.series_id,
                destination_name: book.destination_name,
                upgrade_book_id: book.upgrade_book_id,
            })
            .collect(),
    }
}

fn interface_task_record(task: ApplicationTaskQueueRecord) -> TaskQueueRecord {
    let mut record = TaskQueueRecord::new(task.id, task.priority, task.group);
    record.simple_type = task.simple_type;
    record.payload = task.payload;
    record.owner = task.owner;
    record.order = task.order;
    record
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::path::PathBuf;

    struct StubImportService;

    impl RuntimeMediaImportService for StubImportService {
        fn enqueue_books(
            &self,
            payload: ApplicationBooksImportPayload,
            next_task_id: &mut dyn FnMut() -> String,
        ) -> Result<Vec<ApplicationTaskQueueRecord>, String> {
            let book = payload
                .books
                .into_iter()
                .next()
                .expect("test payload should contain one book");
            Ok(vec![ApplicationTaskQueueRecord::new(
                next_task_id(),
                100,
                Some(book.series_id),
            )])
        }

        fn process_queued_book_payload<'a>(
            &'a self,
            _task_payload: &'a str,
            _import_priority: i32,
        ) -> futures_util::future::BoxFuture<'a, Result<Vec<ApplicationTaskQueueRecord>, String>> {
            Box::pin(async { panic!("process_queued_book_payload should not be called in tests") })
        }
    }

    #[test]
    fn enqueue_books_best_effort_keeps_later_books_when_earlier_enqueue_fails() {
        let service = StubImportService;
        let payload = BooksImportPayload {
            copy_mode: ImportCopyMode::Copy,
            books: vec![
                BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-a.cbz"),
                    series_id: "series-1".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                },
                BooksImportEntry {
                    source_file: PathBuf::from("/tmp/book-b.cbz"),
                    series_id: "series-2".to_string(),
                    destination_name: None,
                    upgrade_book_id: None,
                },
            ],
        };
        let persisted_ids = RefCell::new(Vec::new());

        enqueue_books_best_effort(
            &service,
            payload,
            |task_record| {
                if task_record.id == "IMPORT_BOOK_series-1_/tmp/book-a.cbz" {
                    Err("first enqueue failed".to_string())
                } else {
                    persisted_ids.borrow_mut().push(task_record.id);
                    Ok(())
                }
            },
        );

        assert_eq!(
            persisted_ids.into_inner(),
            vec!["IMPORT_BOOK_series-2_/tmp/book-b.cbz".to_string()]
        );
    }
}
