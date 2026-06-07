use super::*;
use crate::helpers::read_progress_validation_error_response;
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
pub use readlists::{readlist_tachiyomi_read_progress_get, readlist_tachiyomi_read_progress_put};
pub use series::{
    series_read_progress_delete, series_read_progress_post, series_tachiyomi_read_progress_get,
    series_tachiyomi_read_progress_put,
};
