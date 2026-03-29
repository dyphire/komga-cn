use crate::common_ids::{BookId, CollectionId, LibraryId, ReadListId, SeriesId};

use super::{Book, Collection, Library, LibraryCatalogEvent, ReadList, Series};

pub trait LibraryCatalogWritePort {
    fn save_library(&self, library: &Library) -> Result<(), String>;
    fn save_series(&self, series: &Series) -> Result<(), String>;
    fn save_book(&self, book: &Book) -> Result<(), String>;
    fn save_collection(&self, collection: &Collection) -> Result<(), String>;
    fn save_read_list(&self, read_list: &ReadList) -> Result<(), String>;

    fn delete_library(&self, library_id: &LibraryId) -> Result<(), String>;
    fn delete_series(&self, series_id: &SeriesId) -> Result<(), String>;
    fn delete_book(&self, book_id: &BookId) -> Result<(), String>;
    fn delete_collection(&self, collection_id: &CollectionId) -> Result<(), String>;
    fn delete_read_list(&self, read_list_id: &ReadListId) -> Result<(), String>;
}

pub trait SeriesMembershipWritePort {
    fn replace_collection_members(
        &self,
        collection_id: &CollectionId,
        series_ids: &[SeriesId],
    ) -> Result<(), String>;

    fn replace_read_list_members(
        &self,
        read_list_id: &ReadListId,
        book_ids: &[BookId],
    ) -> Result<(), String>;
}

pub trait LibraryCatalogEventPublisher {
    fn publish(&self, event: &LibraryCatalogEvent) -> Result<(), String>;
}
