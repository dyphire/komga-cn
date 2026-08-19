use crate::contracts::library_catalog::LibraryDto;
use crate::helpers::spring_error_response;
use crate::state::LibraryCatalogState;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::library_catalog::{LibraryCatalogMutationError, LibraryChangeSet};
use komga_application::task_processing::{
    SubmitUrgency, TaskQueueRecord as ApplicationTaskQueueRecord,
};

pub(super) struct LibraryCatalogCommands<'a> {
    app: &'a LibraryCatalogState,
}

impl<'a> LibraryCatalogCommands<'a> {
    pub(super) fn new(app: &'a LibraryCatalogState) -> Self {
        Self { app }
    }

    pub(super) async fn create_library(&self, changes: LibraryChangeSet) -> Response {
        match self.app.library_catalog.create_library(changes).await {
            Ok(result) => {
                let enqueue_response = enqueue_task_records(self.app, result.task_records).await;
                if enqueue_response.status().is_server_error() {
                    return enqueue_response;
                }
                Json(LibraryDto::from_record(&result.library, true)).into_response()
            }
            Err(error) => mutation_error_response(error),
        }
    }

    pub(super) async fn update_library(
        &self,
        library_id: &str,
        changes: LibraryChangeSet,
    ) -> Response {
        match self
            .app
            .library_catalog
            .update_library(library_id, changes)
            .await
        {
            Ok(result) if result.task_records.is_empty() => StatusCode::NO_CONTENT.into_response(),
            Ok(result) => {
                enqueue_task_records_with_status(
                    self.app,
                    result.task_records,
                    StatusCode::NO_CONTENT,
                )
                .await
            }
            Err(error) => mutation_error_response(error),
        }
    }

    pub(super) async fn delete_library(&self, library_id: &str) -> Response {
        match self.app.library_catalog.delete_library(library_id).await {
            Ok(true) => StatusCode::NO_CONTENT.into_response(),
            Ok(false) => StatusCode::NOT_FOUND.into_response(),
            Err(error) => mutation_error_response(error),
        }
    }

    pub(super) async fn scan_library(&self, library_id: &str, deep_scan: bool) -> Response {
        match self
            .app
            .library_catalog
            .scan_library(library_id, deep_scan)
            .await
        {
            Ok(result) => enqueue_task_records(self.app, result.task_records).await,
            Err(error) => mutation_error_response(error),
        }
    }

    pub(super) async fn analyze_library(&self, library_id: &str) -> Response {
        match self.app.library_catalog.analyze_library(library_id).await {
            Ok(result) => enqueue_task_records(self.app, result.task_records).await,
            Err(error) => mutation_error_response(error),
        }
    }

    pub(super) async fn refresh_metadata(&self, library_id: &str) -> Response {
        match self.app.library_catalog.refresh_metadata(library_id).await {
            Ok(result) => enqueue_task_records(self.app, result.task_records).await,
            Err(error) => mutation_error_response(error),
        }
    }

    pub(super) async fn empty_trash(&self, library_id: &str) -> Response {
        match self.app.library_catalog.empty_trash(library_id).await {
            Ok(result) => enqueue_task_records(self.app, result.task_records).await,
            Err(error) => mutation_error_response(error),
        }
    }
}

async fn enqueue_task_records(
    app: &LibraryCatalogState,
    task_records: Vec<ApplicationTaskQueueRecord>,
) -> Response {
    enqueue_task_records_with_status(app, task_records, StatusCode::ACCEPTED).await
}

async fn enqueue_task_records_with_status(
    app: &LibraryCatalogState,
    task_records: Vec<ApplicationTaskQueueRecord>,
    status: StatusCode,
) -> Response {
    if let Err(error) = app
        .task_queue
        .queue
        .enqueue_records(task_records, SubmitUrgency::Immediate)
        .await
    {
        tracing::error!(?error, "library catalog task enqueue failed");
        return spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"));
    }

    status.into_response()
}

fn mutation_error_response(error: LibraryCatalogMutationError) -> Response {
    match error {
        LibraryCatalogMutationError::NotFound => StatusCode::NOT_FOUND.into_response(),
        LibraryCatalogMutationError::Validation(message) => bad_request_response(&message),
        LibraryCatalogMutationError::Persistence(message) => internal_error_response(message),
    }
}

fn bad_request_response(message: &str) -> Response {
    spring_error_response(StatusCode::BAD_REQUEST, message)
}

fn internal_error_response(error: impl std::fmt::Display + std::fmt::Debug) -> Response {
    tracing::error!(?error, "library catalog mutation failed");
    spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"))
}
