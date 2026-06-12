use crate::common_ids::{BookId, CollectionId, LibraryId, ReadListId, SeriesId};
use crate::media_assets::ThumbnailType;

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
    pub thumbnail_type: Option<ThumbnailType>,
    pub selected: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ReadStatusCondition {
    Include(Vec<ReadStatus>),
    Exclude(Vec<ReadStatus>),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ReadStatus {
    Read,
    InProgress,
    Unread,
}

impl ReadStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "read" => Some(Self::Read),
            "in_progress" | "inprogress" => Some(Self::InProgress),
            "unread" => Some(Self::Unread),
            _ => None,
        }
    }

    pub fn from_progress(page: i32, completed: bool) -> Self {
        if completed {
            Self::Read
        } else if page > 0 {
            Self::InProgress
        } else {
            Self::Unread
        }
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Read => "READ",
            Self::InProgress => "IN_PROGRESS",
            Self::Unread => "UNREAD",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MediaProfile {
    Divina,
    Epub,
    Pdf,
}

impl MediaProfile {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "divina" => Some(Self::Divina),
            "epub" => Some(Self::Epub),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    pub fn from_media_type(media_type: &str) -> Option<Self> {
        match media_type {
            "application/zip"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5" => Some(Self::Divina),
            "application/epub+zip" => Some(Self::Epub),
            "application/pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    pub fn api_name(self) -> &'static str {
        match self {
            Self::Divina => "DIVINA",
            Self::Epub => "EPUB",
            Self::Pdf => "PDF",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MediaStatus {
    Ready,
    Unknown,
    Error,
    Unsupported,
    Outdated,
}

impl MediaStatus {
    const VALUES: [Self; 5] = [
        Self::Ready,
        Self::Unknown,
        Self::Error,
        Self::Unsupported,
        Self::Outdated,
    ];

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "READY" => Some(Self::Ready),
            "UNKNOWN" => Some(Self::Unknown),
            "ERROR" => Some(Self::Error),
            "UNSUPPORTED" => Some(Self::Unsupported),
            "OUTDATED" => Some(Self::Outdated),
            _ => None,
        }
    }

    pub fn matching_persisted_name_prefix(prefix: &str) -> Vec<Self> {
        let prefix = prefix.trim().to_ascii_uppercase();
        if prefix.is_empty() {
            return Vec::new();
        }

        Self::VALUES
            .into_iter()
            .filter(|status| status.persisted_name().starts_with(&prefix))
            .collect()
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Ready => "READY",
            Self::Unknown => "UNKNOWN",
            Self::Error => "ERROR",
            Self::Unsupported => "UNSUPPORTED",
            Self::Outdated => "OUTDATED",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SeriesStatus {
    Ended,
    Ongoing,
    Abandoned,
    Hiatus,
}

impl SeriesStatus {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "ENDED" => Some(Self::Ended),
            "ONGOING" => Some(Self::Ongoing),
            "ABANDONED" => Some(Self::Abandoned),
            "HIATUS" => Some(Self::Hiatus),
            _ => None,
        }
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Ended => "ENDED",
            Self::Ongoing => "ONGOING",
            Self::Abandoned => "ABANDONED",
            Self::Hiatus => "HIATUS",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SeriesStatusCondition {
    Include(Vec<SeriesStatus>),
    Exclude(Vec<SeriesStatus>),
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
    MediaProfile(InclusionCondition<MediaProfile>),
    MediaStatus(InclusionCondition<MediaStatus>),
    Author(StringCondition),
    Poster(InclusionCondition<BookPosterCondition>),
    NumberSort(NumberCondition),
    ReleaseDate(DateCondition),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_status_parses_and_renders_persisted_names() {
        let cases = [
            ("ENDED", SeriesStatus::Ended, "ENDED"),
            ("ongoing", SeriesStatus::Ongoing, "ONGOING"),
            ("Abandoned", SeriesStatus::Abandoned, "ABANDONED"),
            ("HIATUS", SeriesStatus::Hiatus, "HIATUS"),
        ];

        for (raw, status, persisted_name) in cases {
            assert_eq!(SeriesStatus::parse(raw), Some(status));
            assert_eq!(status.persisted_name(), persisted_name);
        }

        assert_eq!(SeriesStatus::parse("BROKEN_STATUS"), None);
    }

    #[test]
    fn read_status_parses_and_renders_persisted_names() {
        let cases = [
            ("READ", ReadStatus::Read, "READ"),
            ("in_progress", ReadStatus::InProgress, "IN_PROGRESS"),
            ("inProgress", ReadStatus::InProgress, "IN_PROGRESS"),
            ("Unread", ReadStatus::Unread, "UNREAD"),
        ];

        for (raw, status, persisted_name) in cases {
            assert_eq!(ReadStatus::parse(raw), Some(status));
            assert_eq!(status.persisted_name(), persisted_name);
        }

        assert_eq!(ReadStatus::parse("BROKEN_STATUS"), None);
    }

    #[test]
    fn media_profile_parses_renders_and_resolves_media_types() {
        let cases = [
            ("DIVINA", MediaProfile::Divina, "DIVINA"),
            ("epub", MediaProfile::Epub, "EPUB"),
            ("Pdf", MediaProfile::Pdf, "PDF"),
        ];

        for (raw, profile, api_name) in cases {
            assert_eq!(MediaProfile::parse(raw), Some(profile));
            assert_eq!(profile.api_name(), api_name);
        }

        assert_eq!(MediaProfile::parse("BROKEN_PROFILE"), None);
        assert_eq!(
            MediaProfile::from_media_type("application/zip"),
            Some(MediaProfile::Divina)
        );
        assert_eq!(
            MediaProfile::from_media_type("application/epub+zip"),
            Some(MediaProfile::Epub)
        );
        assert_eq!(
            MediaProfile::from_media_type("application/pdf"),
            Some(MediaProfile::Pdf)
        );
        assert_eq!(
            MediaProfile::from_media_type("application/octet-stream"),
            None
        );
    }

    #[test]
    fn media_status_parses_renders_and_matches_prefixes() {
        let cases = [
            ("READY", MediaStatus::Ready, "READY"),
            ("unknown", MediaStatus::Unknown, "UNKNOWN"),
            ("Error", MediaStatus::Error, "ERROR"),
            ("UNSUPPORTED", MediaStatus::Unsupported, "UNSUPPORTED"),
            ("outdated", MediaStatus::Outdated, "OUTDATED"),
        ];

        for (raw, status, persisted_name) in cases {
            assert_eq!(MediaStatus::parse(raw), Some(status));
            assert_eq!(status.persisted_name(), persisted_name);
        }

        assert_eq!(MediaStatus::parse("BROKEN_STATUS"), None);
        assert_eq!(
            MediaStatus::matching_persisted_name_prefix("un"),
            vec![MediaStatus::Unknown, MediaStatus::Unsupported],
        );
        assert!(MediaStatus::matching_persisted_name_prefix("BROKEN").is_empty());
    }
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
