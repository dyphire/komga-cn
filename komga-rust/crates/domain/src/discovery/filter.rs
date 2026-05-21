use crate::common_ids::{BookId, CollectionId, LibraryId, ReadListId, SeriesId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterOperator {
    All,
    Any,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositeSeriesCondition {
    pub operator: FilterOperator,
    pub conditions: Vec<SeriesCondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositeBookCondition {
    pub operator: FilterOperator,
    pub conditions: Vec<BookCondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InclusionCondition<T> {
    Include(Vec<T>),
    Exclude(Vec<T>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StringCondition {
    Exact(InclusionCondition<String>),
    Contains(InclusionCondition<String>),
    StartsWith(InclusionCondition<String>),
    EndsWith(InclusionCondition<String>),
    Regex(Vec<String>),
    IsEmpty,
    IsNotEmpty,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DateCondition {
    Exact(InclusionCondition<String>),
    Before(String),
    After(String),
    Contains(InclusionCondition<String>),
    StartsWith(InclusionCondition<String>),
    EndsWith(InclusionCondition<String>),
    WithinLastDays(i64),
    OutsideLastDays(i64),
    IsEmpty,
    IsNotEmpty,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NumberCondition {
    Exact(InclusionCondition<String>),
    GreaterThan(String),
    LessThan(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AgeRatingCondition {
    Exact(InclusionCondition<u16>),
    ExactOrEmpty(Vec<u16>),
    GreaterThan(u16),
    LessThan(u16),
    IsEmpty,
    IsNotEmpty,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookPosterCondition {
    pub thumbnail_type: Option<String>,
    pub selected: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadStatusCondition {
    Include(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SeriesStatusCondition {
    Include(Vec<String>),
    Exclude(Vec<String>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SeriesValueCondition {
    LibraryId(InclusionCondition<LibraryId>),
    CollectionId(InclusionCondition<CollectionId>),
    Title(StringCondition),
    TitleSort(StringCondition),
    Deleted(bool),
    OneShot(bool),
    ReadStatus(ReadStatusCondition),
    Genre(StringCondition),
    Tag(StringCondition),
    Language(InclusionCondition<String>),
    Publisher(InclusionCondition<String>),
    AgeRating(AgeRatingCondition),
    ReleaseDate(DateCondition),
    SharingLabel(StringCondition),
    SeriesStatus(SeriesStatusCondition),
    Complete(bool),
    Author(StringCondition),
    ExcludeNewlyAdded(bool),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookValueCondition {
    LibraryId(InclusionCondition<LibraryId>),
    SeriesId(InclusionCondition<SeriesId>),
    ReadListId(InclusionCondition<ReadListId>),
    Title(StringCondition),
    Deleted(bool),
    OneShot(bool),
    Tag(StringCondition),
    Genre(StringCondition),
    Language(InclusionCondition<String>),
    Publisher(InclusionCondition<String>),
    AgeRating(InclusionCondition<u16>),
    ReadStatus(ReadStatusCondition),
    MediaProfile(InclusionCondition<String>),
    MediaStatus(InclusionCondition<String>),
    Author(StringCondition),
    Poster(InclusionCondition<BookPosterCondition>),
    NumberSort(NumberCondition),
    ReleaseDate(DateCondition),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SeriesCondition {
    Composite(CompositeSeriesCondition),
    Value(SeriesValueCondition),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookCondition {
    Composite(CompositeBookCondition),
    Value(BookValueCondition),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesFilter {
    pub condition: Option<SeriesCondition>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BookFilter {
    pub condition: Option<BookCondition>,
    pub direct_browse_book_id: Option<BookId>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoverySavedSearch {
    pub name: String,
    pub series_filter: Option<SeriesFilter>,
    pub book_filter: Option<BookFilter>,
}
