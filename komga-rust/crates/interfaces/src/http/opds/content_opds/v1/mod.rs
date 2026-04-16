use super::feeds::normalize_opds_updated;
use super::types::{PersistedBookFeedItem, PersistedSeriesBook};
use super::*;
use crate::media_assets_runtime_access::facade::{
    load_archive_page_rows, load_persisted_book_media, load_persisted_book_pages,
};
use komga_application::media_assets::content_type_from_filename;
use time::format_description::well_known::Rfc3339;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime, UtcOffset};

mod browse;
mod details;
mod helpers;
mod streaming;

pub(crate) use browse::{
    opds_v1_books_latest, opds_v1_catalog, opds_v1_collections, opds_v1_keep_reading,
    opds_v1_libraries, opds_v1_on_deck, opds_v1_publishers, opds_v1_readlists, opds_v1_search,
    opds_v1_series, opds_v1_series_latest,
};
pub(crate) use details::{
    opds_v1_collection_detail, opds_v1_library_detail, opds_v1_readlist_detail,
    opds_v1_series_detail,
};

pub(super) fn opds_v1_basic_unauthorized_response() -> Response {
    helpers::opds_v1_basic_unauthorized_response()
}
