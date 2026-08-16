use komga_domain::media_assets::ThumbnailType;

use super::{
    BookMediaPort, BookMediaRecord, EntityExistencePort, SeriesRelationPort, ThumbnailReadPort,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityThumbnailRecord {
    pub id: String,
    pub book_id: String,
    pub thumbnail_type: ThumbnailType,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityThumbnailBinary {
    pub owner_id: String,
    pub thumbnail_type: ThumbnailType,
    pub media_type: String,
    pub thumbnail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesThumbnailRecord {
    pub id: String,
    pub series_id: String,
    pub thumbnail_type: ThumbnailType,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlistThumbnailRecord {
    pub id: String,
    pub readlist_id: String,
    pub thumbnail_type: ThumbnailType,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumbnail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionThumbnailRecord {
    pub id: String,
    pub collection_id: String,
    pub thumbnail_type: ThumbnailType,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumbnail: Vec<u8>,
}

#[async_trait::async_trait]
pub trait ThumbnailReaderPort: Send + Sync {
    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;

    async fn book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;

    async fn book_thumbnails(&self, book_id: &str) -> anyhow::Result<Vec<EntityThumbnailRecord>>;

    async fn selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;

    async fn series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>>;

    async fn series_thumbnails(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<SeriesThumbnailRecord>>;

    async fn readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<ReadlistThumbnailRecord>>;

    async fn readlist_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<ReadlistThumbnailRecord>>;

    async fn collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<CollectionThumbnailRecord>>;

    async fn collection_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<CollectionThumbnailRecord>>;

    async fn book_media(&self, book_id: &str) -> anyhow::Result<Option<BookMediaRecord>>;

    async fn book_exists(&self, book_id: &str) -> anyhow::Result<bool>;

    async fn series_exists(&self, series_id: &str) -> anyhow::Result<bool>;

    async fn readlist_exists(&self, readlist_id: &str) -> anyhow::Result<bool>;

    async fn collection_exists(&self, collection_id: &str) -> anyhow::Result<bool>;

    async fn series_oneshot(&self, series_id: &str) -> anyhow::Result<Option<bool>>;

    async fn series_book_ids(&self, series_id: &str) -> anyhow::Result<Vec<String>>;
}

#[async_trait::async_trait]
impl<T> ThumbnailReaderPort for T
where
    T: ThumbnailReadPort
        + BookMediaPort
        + EntityExistencePort
        + SeriesRelationPort
        + Send
        + Sync
        + ?Sized,
{
    async fn selected_book_thumbnail(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>> {
        ThumbnailReadPort::selected_book_thumbnail(self, book_id).await
    }

    async fn book_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>> {
        ThumbnailReadPort::book_thumbnail_by_id(self, thumbnail_id).await
    }

    async fn book_thumbnails(&self, book_id: &str) -> anyhow::Result<Vec<EntityThumbnailRecord>> {
        ThumbnailReadPort::book_thumbnails(self, book_id).await
    }

    async fn selected_series_thumbnail(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>> {
        ThumbnailReadPort::selected_series_thumbnail(self, series_id).await
    }

    async fn series_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<EntityThumbnailBinary>> {
        ThumbnailReadPort::series_thumbnail_by_id(self, thumbnail_id).await
    }

    async fn series_thumbnails(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Vec<SeriesThumbnailRecord>> {
        ThumbnailReadPort::series_thumbnails(self, series_id).await
    }

    async fn readlist_thumbnails(
        &self,
        readlist_id: &str,
    ) -> anyhow::Result<Vec<ReadlistThumbnailRecord>> {
        ThumbnailReadPort::readlist_thumbnails(self, readlist_id).await
    }

    async fn readlist_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<ReadlistThumbnailRecord>> {
        ThumbnailReadPort::readlist_thumbnail_by_id(self, thumbnail_id).await
    }

    async fn collection_thumbnails(
        &self,
        collection_id: &str,
    ) -> anyhow::Result<Vec<CollectionThumbnailRecord>> {
        ThumbnailReadPort::collection_thumbnails(self, collection_id).await
    }

    async fn collection_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> anyhow::Result<Option<CollectionThumbnailRecord>> {
        ThumbnailReadPort::collection_thumbnail_by_id(self, thumbnail_id).await
    }

    async fn book_media(&self, book_id: &str) -> anyhow::Result<Option<BookMediaRecord>> {
        BookMediaPort::book_media(self, book_id).await
    }

    async fn book_exists(&self, book_id: &str) -> anyhow::Result<bool> {
        EntityExistencePort::book_exists(self, book_id).await
    }

    async fn series_exists(&self, series_id: &str) -> anyhow::Result<bool> {
        EntityExistencePort::series_exists(self, series_id).await
    }

    async fn readlist_exists(&self, readlist_id: &str) -> anyhow::Result<bool> {
        EntityExistencePort::readlist_exists(self, readlist_id).await
    }

    async fn collection_exists(&self, collection_id: &str) -> anyhow::Result<bool> {
        EntityExistencePort::collection_exists(self, collection_id).await
    }

    async fn series_oneshot(&self, series_id: &str) -> anyhow::Result<Option<bool>> {
        SeriesRelationPort::series_oneshot(self, series_id).await
    }

    async fn series_book_ids(&self, series_id: &str) -> anyhow::Result<Vec<String>> {
        SeriesRelationPort::series_book_ids(self, series_id).await
    }
}
