use super::{Book, Collection, Library, ReadList, Series};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LibraryCatalogEvent {
    LibraryAdded(Library),
    LibraryUpdated(Library),
    LibraryDeleted(Library),
    SeriesAdded(Series),
    SeriesUpdated(Series),
    SeriesDeleted(Series),
    BookAdded(Book),
    BookUpdated(Book),
    BookDeleted(Book),
    CollectionAdded(Collection),
    CollectionUpdated(Collection),
    CollectionDeleted(Collection),
    ReadListAdded(ReadList),
    ReadListUpdated(ReadList),
    ReadListDeleted(ReadList),
}
