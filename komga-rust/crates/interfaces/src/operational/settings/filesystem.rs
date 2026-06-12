use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use komga_application::operational::{
    FilesystemBrowseError, FilesystemBrowseRequest, FilesystemDirectoryListing, FilesystemEntry,
    FilesystemEntryType,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

use crate::identity_access::auth::Admin;
use crate::state::OperationalApiState;

#[derive(Default, Deserialize)]
#[serde(default)]
struct DirectoryRequestDto {
    path: String,
    #[serde(rename = "showFiles")]
    show_files: bool,
}

pub(crate) async fn post_filesystem(
    State(app): State<OperationalApiState>,
    _: Admin,
    body: Bytes,
) -> Response {
    let request = if body.is_empty() {
        DirectoryRequestDto::default()
    } else {
        match serde_json::from_slice::<DirectoryRequestDto>(&body) {
            Ok(value) => value,
            Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };

    match app.filesystem_browse.browse(FilesystemBrowseRequest {
        path: request.path,
        show_files: request.show_files,
    }) {
        Ok(listing) => Json(directory_listing_payload(listing)).into_response(),
        Err(FilesystemBrowseError::BadRequest) => StatusCode::BAD_REQUEST.into_response(),
        Err(FilesystemBrowseError::Internal) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn directory_listing_payload(listing: FilesystemDirectoryListing) -> Value {
    let mut payload = Map::new();
    if let Some(parent) = listing.parent {
        payload.insert("parent".to_string(), Value::String(parent));
    }
    payload.insert(
        "directories".to_string(),
        Value::Array(
            listing
                .directories
                .into_iter()
                .map(directory_entry_payload)
                .collect(),
        ),
    );
    payload.insert(
        "files".to_string(),
        Value::Array(
            listing
                .files
                .into_iter()
                .map(directory_entry_payload)
                .collect(),
        ),
    );
    Value::Object(payload)
}

fn directory_entry_payload(entry: FilesystemEntry) -> Value {
    json!({
        "type": match entry.entry_type {
            FilesystemEntryType::Directory => "directory",
            FilesystemEntryType::File => "file",
        },
        "name": entry.name,
        "path": entry.path,
    })
}
