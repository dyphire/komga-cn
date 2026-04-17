use super::*;
use komga_application::media_assets::{
    BooksImportEntry as ApplicationBooksImportEntry,
    BooksImportPayload as ApplicationBooksImportPayload,
    ImportCopyMode as ApplicationImportCopyMode, generate_prefixed_id,
};
use komga_application::task_processing::TaskQueueRecord as ApplicationTaskQueueRecord;

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
    let mut next_task_id = || format!("IMPORT_BOOK:{}", generate_prefixed_id("import-book"));
    let task_records =
        match service.enqueue_books(application_import_payload(payload), &mut next_task_id) {
            Ok(task_records) => task_records,
            Err(error) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({ "error": error })),
                )
                    .into_response();
            }
        };

    enqueue_task_records(
        &state,
        task_records
            .into_iter()
            .map(interface_task_record)
            .collect(),
    )
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
