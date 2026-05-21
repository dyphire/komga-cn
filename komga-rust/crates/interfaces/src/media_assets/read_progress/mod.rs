use super::*;
use crate::helpers::read_progress_validation_error_response;
use crate::state::MediaAssetsState;
use crate::state::PersistedReadProgressRecord;
use flate2::read::GzDecoder;
use std::io::Read;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

mod books;
mod epub;
mod readlists;
mod series;

const READIUM_PROGRESSION_MEDIA_TYPE: &str = "application/vnd.readium.progression+json";

pub use books::{
    book_progression, book_progression_get, book_read_progress, book_read_progress_delete,
    opds_v2_book_progression, opds_v2_book_progression_get,
};
pub(crate) use epub::{normalize_book_epub_locator, progression_is_older_than_existing};
pub use readlists::{readlist_tachiyomi_read_progress_get, readlist_tachiyomi_read_progress_put};
pub use series::{
    series_read_progress_delete, series_read_progress_post, series_tachiyomi_read_progress_get,
    series_tachiyomi_read_progress_put,
};
pub(crate) async fn load_read_progress_from_services(
    app: &MediaAssetsState,
    book_id: &str,
    user_id: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
    app.identity.load_read_progress(book_id, user_id).await
}

pub(crate) async fn load_series_book_ids_from_services(
    app: &MediaAssetsState,
    series_id: &str,
) -> Result<Vec<String>, String> {
    app.reader.series_book_ids(series_id).await
}

pub(crate) async fn persist_read_progress_from_services(
    app: &MediaAssetsState,
    book_id: &str,
    user_id: &str,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Result<(), String> {
    app.progress
        .persist_read_progress(book_id, user_id, page, completed, locator)
        .await
}

pub(crate) async fn delete_persisted_read_progress_from_services(
    app: &MediaAssetsState,
    book_id: &str,
    user_id: &str,
) -> Result<(), String> {
    app.progress.delete_read_progress(book_id, user_id).await
}

pub(crate) async fn refresh_series_read_progress_row_from_services(
    app: &MediaAssetsState,
    series_id: &str,
    user_id: &str,
) -> Result<(), String> {
    app.progress
        .refresh_series_read_progress(series_id, user_id)
        .await
}

pub(crate) async fn delete_series_read_progress_row_from_services(
    app: &MediaAssetsState,
    series_id: &str,
    user_id: &str,
) -> Result<(), String> {
    app.progress
        .delete_series_read_progress(series_id, user_id)
        .await
}

pub(crate) async fn load_series_tachiyomi_progress_from_services(
    app: &MediaAssetsState,
    series_id: &str,
    user_id: &str,
) -> Result<Option<Value>, String> {
    app.reader
        .series_tachiyomi_progress(series_id, user_id)
        .await
}
