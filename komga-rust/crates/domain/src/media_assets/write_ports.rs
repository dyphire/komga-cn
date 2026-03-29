use crate::common_ids::{BookId, SeriesId};

use super::{BookMediaAsset, SeriesThumbnail};

pub trait BookMediaAssetWritePort {
    fn save_asset(&self, asset: &BookMediaAsset) -> Result<(), String>;
    fn delete_assets_for_book(&self, book_id: &BookId) -> Result<(), String>;
}

pub trait SeriesThumbnailWritePort {
    fn save_thumbnail(&self, thumbnail: &SeriesThumbnail) -> Result<(), String>;
    fn delete_thumbnails_for_series(&self, series_id: &SeriesId) -> Result<(), String>;
}
