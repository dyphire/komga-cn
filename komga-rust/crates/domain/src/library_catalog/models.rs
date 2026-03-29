use crate::common_ids::{BookId, CollectionId, LibraryId, ReadListId, SeriesId};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Library {
    pub id: LibraryId,
    pub name: String,
    pub root: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Series {
    pub id: SeriesId,
    pub library_id: LibraryId,
    pub title: String,
    pub deleted: bool,
    pub one_shot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Book {
    pub id: BookId,
    pub series_id: SeriesId,
    pub library_id: LibraryId,
    pub title: String,
    pub deleted: bool,
    pub one_shot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Collection {
    pub id: CollectionId,
    pub name: String,
    pub ordered: bool,
    pub series_ids: Vec<SeriesId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadList {
    pub id: ReadListId,
    pub name: String,
    pub ordered: bool,
    pub book_ids: Vec<BookId>,
}
