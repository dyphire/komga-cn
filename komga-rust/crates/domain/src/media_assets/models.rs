use crate::common_ids::{BookId, SeriesId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaAssetType {
    Thumbnail,
    Page,
    File,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookMediaAsset {
    pub book_id: BookId,
    pub asset_type: MediaAssetType,
    pub locator: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesThumbnail {
    pub series_id: SeriesId,
    pub locator: String,
    pub selected: bool,
}
