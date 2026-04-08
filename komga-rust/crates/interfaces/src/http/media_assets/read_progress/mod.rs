use super::*;
use crate::http::helpers::read_progress_validation_error_response;
use crate::opds_persisted_access::load_readlist_books;
use crate::runtime_identity_access::load_read_progress;
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
