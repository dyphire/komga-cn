use super::*;
use crate::helpers::read_progress_validation_error_response;
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
