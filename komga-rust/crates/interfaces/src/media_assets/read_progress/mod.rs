use super::*;
use crate::helpers::read_progress_validation_error_response;
use crate::state::HttpAppState;
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
};
pub(crate) use epub::{normalize_book_epub_locator, progression_is_older_than_existing};
pub use readlists::{readlist_tachiyomi_read_progress_get, readlist_tachiyomi_read_progress_put};
pub use series::{
    series_read_progress_delete, series_read_progress_post, series_tachiyomi_read_progress_get,
    series_tachiyomi_read_progress_put,
};
pub(crate) async fn load_read_progress_from_services(
    app: &HttpAppState,
    book_id: &str,
    user_id: &str,
) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
    app.services
        .runtime_identity
        .load_read_progress(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            user_id.to_string(),
        )
        .await
}

pub(crate) async fn load_series_book_ids_from_services(
    app: &HttpAppState,
    series_id: &str,
) -> Result<Vec<String>, String> {
    app.services
        .media_assets
        .load_series_book_ids(app.auth_db.database_file.clone(), series_id.to_string())
        .await
}

pub(crate) async fn persist_read_progress_from_services(
    app: &HttpAppState,
    book_id: &str,
    user_id: &str,
    page: u64,
    completed: bool,
    locator: Option<Value>,
) -> Result<(), String> {
    app.services
        .media_assets
        .persist_read_progress(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            user_id.to_string(),
            page,
            completed,
            locator,
        )
        .await
}

pub(crate) async fn delete_persisted_read_progress_from_services(
    app: &HttpAppState,
    book_id: &str,
    user_id: &str,
) -> Result<(), String> {
    app.services
        .media_assets
        .delete_persisted_read_progress(
            app.auth_db.database_file.clone(),
            book_id.to_string(),
            user_id.to_string(),
        )
        .await
}

pub(crate) async fn refresh_series_read_progress_row_from_services(
    app: &HttpAppState,
    series_id: &str,
    user_id: &str,
) -> Result<(), String> {
    app.services
        .media_assets
        .refresh_series_read_progress_row(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            user_id.to_string(),
        )
        .await
}

pub(crate) async fn delete_series_read_progress_row_from_services(
    app: &HttpAppState,
    series_id: &str,
    user_id: &str,
) -> Result<(), String> {
    app.services
        .media_assets
        .delete_series_read_progress_row(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            user_id.to_string(),
        )
        .await
}

pub(crate) async fn load_series_tachiyomi_progress_from_services(
    app: &HttpAppState,
    series_id: &str,
    user_id: &str,
) -> Result<Option<Value>, String> {
    app.services
        .media_assets
        .load_series_tachiyomi_progress(
            app.auth_db.database_file.clone(),
            series_id.to_string(),
            user_id.to_string(),
        )
        .await
}
