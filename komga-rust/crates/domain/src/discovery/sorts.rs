#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesSort {
    MetadataTitleSort,
    CreatedDate,
    LastModifiedDate,
    BooksMetadataReleaseDate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookSort {
    MetadataTitle,
    CreatedDate,
    LastModifiedDate,
    MetadataReleaseDate,
}
