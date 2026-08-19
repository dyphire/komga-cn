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

use crate::contracts::filesystem::{
    DirectoryListingDto, FilesystemEntryDto, FilesystemEntryTypeDto,
};
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
        Ok(listing) => Json(directory_listing_dto(listing)).into_response(),
        Err(FilesystemBrowseError::BadRequest) => StatusCode::BAD_REQUEST.into_response(),
        Err(FilesystemBrowseError::Internal) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

fn directory_listing_dto(listing: FilesystemDirectoryListing) -> DirectoryListingDto {
    DirectoryListingDto {
        parent: listing.parent,
        directories: listing
            .directories
            .into_iter()
            .map(directory_entry_dto)
            .collect(),
        files: listing.files.into_iter().map(directory_entry_dto).collect(),
    }
}

fn directory_entry_dto(entry: FilesystemEntry) -> FilesystemEntryDto {
    FilesystemEntryDto {
        entry_type: match entry.entry_type {
            FilesystemEntryType::Directory => FilesystemEntryTypeDto::Directory,
            FilesystemEntryType::File => FilesystemEntryTypeDto::File,
        },
        name: entry.name,
        path: entry.path,
    }
}
