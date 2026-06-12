mod books;
mod epub;
mod readlists;
mod series;

const READIUM_PROGRESSION_MEDIA_TYPE: &str = "application/vnd.readium.progression+json";

pub(crate) use books::{
    book_progression, book_progression_get, book_read_progress, book_read_progress_delete,
    opds_v2_book_progression, opds_v2_book_progression_get,
};
pub(crate) use readlists::{
    readlist_tachiyomi_read_progress_get, readlist_tachiyomi_read_progress_put,
};
pub(crate) use series::{
    series_read_progress_delete, series_read_progress_post, series_tachiyomi_read_progress_get,
    series_tachiyomi_read_progress_put,
};
