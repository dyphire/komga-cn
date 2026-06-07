use super::*;
use crate::media_assets::access_control::user_can_access_book_media;
use crate::media_assets::http_helpers::attachment_disposition;
use crate::state::IdentityAccessState;
use axum::extract::State;
mod catch_all;
mod common;
mod files;
mod library_sync;
mod metadata;
mod proxy;
mod reading_state;
mod thumbnails;

pub use catch_all::kobo_catch_all;
pub(in crate::identity_access::device_auth) use catch_all::proxy_kobo_catch_all_request;
pub use files::kobo_book_file_epub;
pub use library_sync::kobo_library_sync;
pub use metadata::kobo_library_book_metadata;
pub use reading_state::{kobo_library_book_state, kobo_library_book_state_update};
pub use thumbnails::{kobo_book_thumbnail, kobo_book_thumbnail_with_quality};

use common::resolved_kobo_request_api_key_metadata;
use proxy::proxied_missing_kobo_book_response;

async fn load_kobo_metadata_record(
    app: &IdentityAccessState,
    book_id: &str,
) -> Result<Option<crate::state::KoboMetadataRecord>, String> {
    app.identity
        .device_sync()
        .load_kobo_metadata_record(book_id)
        .await
}

async fn persisted_book_exists(app: &IdentityAccessState, book_id: &str) -> Result<bool, String> {
    app.identity
        .device_sync()
        .persisted_book_exists(book_id)
        .await
}

async fn load_book_created_timestamp(
    app: &IdentityAccessState,
    book_id: &str,
) -> Result<Option<String>, String> {
    app.identity
        .device_sync()
        .load_book_created_timestamp(book_id)
        .await
}
