use language_tags::LanguageTag;
use std::fmt;
use url::Url;

use super::{
    ExistingSeriesMetadataRecord, SeriesAlternateTitleRecord, SeriesMetadataLinkRecord,
    SeriesMetadataUpdateRecord, SeriesReadingDirection,
};
use komga_domain::discovery::SeriesStatus;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SeriesMetadataPatch {
    pub status: Option<SeriesStatus>,
    pub status_lock: Option<bool>,
    pub title: Option<String>,
    pub title_lock: Option<bool>,
    pub title_sort: Option<String>,
    pub title_sort_lock: Option<bool>,
    pub summary: Option<String>,
    pub summary_lock: Option<bool>,
    pub reading_direction: Option<Option<SeriesReadingDirection>>,
    pub reading_direction_lock: Option<bool>,
    pub publisher: Option<String>,
    pub publisher_lock: Option<bool>,
    pub age_rating: Option<Option<u32>>,
    pub age_rating_lock: Option<bool>,
    pub language: Option<String>,
    pub language_lock: Option<bool>,
    pub genres: Option<Vec<String>>,
    pub genres_lock: Option<bool>,
    pub tags: Option<Vec<String>>,
    pub tags_lock: Option<bool>,
    pub total_book_count: Option<Option<u32>>,
    pub total_book_count_lock: Option<bool>,
    pub sharing_labels: Option<Vec<String>>,
    pub sharing_labels_lock: Option<bool>,
    pub links: Option<Vec<SeriesMetadataLinkRecord>>,
    pub links_lock: Option<bool>,
    pub alternate_titles: Option<Vec<SeriesAlternateTitleRecord>>,
    pub alternate_titles_lock: Option<bool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SeriesMetadataUpdateResult {
    Updated,
    NotFound,
}

#[derive(Debug)]
pub enum SeriesMetadataUpdateError {
    Validation(String),
    Persistence(anyhow::Error),
}

impl SeriesMetadataUpdateError {
    pub(crate) fn validation(message: impl Into<String>) -> Self {
        Self::Validation(message.into())
    }

    pub(crate) fn persistence(error: anyhow::Error) -> Self {
        Self::Persistence(error)
    }
}

impl fmt::Display for SeriesMetadataUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Validation(message) => f.write_str(message),
            Self::Persistence(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SeriesMetadataUpdateError {}

#[cfg(test)]
impl PartialEq for SeriesMetadataUpdateError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Validation(left), Self::Validation(right)) => left == right,
            (Self::Persistence(left), Self::Persistence(right)) => {
                left.to_string() == right.to_string()
            }
            _ => false,
        }
    }
}

pub trait SeriesEventEmitter: Send + Sync {
    fn emit_series_changed(&self, series_id: &str, library_id: &str);
}

#[async_trait::async_trait]
pub trait SeriesMetadataWritePort: Send + Sync {
    async fn load_series_library_id(&self, series_id: &str) -> anyhow::Result<Option<String>>;

    async fn load_existing_series_metadata(
        &self,
        series_id: &str,
    ) -> anyhow::Result<Option<ExistingSeriesMetadataRecord>>;

    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> anyhow::Result<bool>;

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> anyhow::Result<()>;
}

pub struct SeriesMetadataWriter<'a, P, E>
where
    P: SeriesMetadataWritePort + ?Sized,
    E: SeriesEventEmitter + ?Sized,
{
    port: &'a P,
    event_emitter: &'a E,
}

impl<'a, P, E> SeriesMetadataWriter<'a, P, E>
where
    P: SeriesMetadataWritePort + ?Sized,
    E: SeriesEventEmitter + ?Sized,
{
    pub fn new(port: &'a P, event_emitter: &'a E) -> Self {
        Self {
            port,
            event_emitter,
        }
    }

    pub async fn update_series(
        &self,
        series_id: &str,
        patch: SeriesMetadataPatch,
    ) -> Result<SeriesMetadataUpdateResult, SeriesMetadataUpdateError> {
        validate_series_metadata_patch(&patch)?;

        let Some(existing) = self
            .port
            .load_existing_series_metadata(series_id)
            .await
            .map_err(SeriesMetadataUpdateError::persistence)?
        else {
            return Ok(SeriesMetadataUpdateResult::NotFound);
        };

        let update = apply_series_metadata_patch(&existing, patch);
        if !self
            .port
            .persist_series_metadata_update(series_id, update)
            .await
            .map_err(SeriesMetadataUpdateError::persistence)?
        {
            return Ok(SeriesMetadataUpdateResult::NotFound);
        }

        if let Some(library_id) = self
            .port
            .load_series_library_id(series_id)
            .await
            .map_err(SeriesMetadataUpdateError::persistence)?
        {
            self.event_emitter
                .emit_series_changed(series_id, &library_id);
        }

        self.port
            .refresh_series_search_documents_after_metadata_update(series_id)
            .await
            .map_err(SeriesMetadataUpdateError::persistence)?;

        Ok(SeriesMetadataUpdateResult::Updated)
    }
}

fn validate_series_metadata_patch(
    patch: &SeriesMetadataPatch,
) -> Result<(), SeriesMetadataUpdateError> {
    validate_optional_non_blank_string(patch.title.as_deref(), "title")?;
    validate_optional_non_blank_string(patch.title_sort.as_deref(), "titleSort")?;

    if let Some(Some(age_rating)) = patch.age_rating
        && age_rating > i32::MAX as u32
    {
        return Err(SeriesMetadataUpdateError::validation(format!(
            "ageRating must be between 0 and {}",
            i32::MAX,
        )));
    }

    if let Some(language) = patch.language.as_deref()
        && !language.trim().is_empty()
        && LanguageTag::parse(language).is_err()
    {
        return Err(SeriesMetadataUpdateError::validation(
            "language must be blank or a valid BCP47 language tag",
        ));
    }

    if let Some(Some(total_book_count)) = patch.total_book_count
        && (total_book_count == 0 || total_book_count > i32::MAX as u32)
    {
        return Err(SeriesMetadataUpdateError::validation(
            "totalBookCount must be a positive integer",
        ));
    }

    if let Some(links) = &patch.links {
        validate_series_metadata_links(links)?;
    }

    if let Some(alternate_titles) = &patch.alternate_titles {
        validate_series_alternate_titles(alternate_titles)?;
    }

    Ok(())
}

fn validate_optional_non_blank_string(
    value: Option<&str>,
    field_name: &str,
) -> Result<(), SeriesMetadataUpdateError> {
    if let Some(value) = value
        && value.trim().is_empty()
    {
        return Err(SeriesMetadataUpdateError::validation(format!(
            "{field_name} must not be blank",
        )));
    }

    Ok(())
}

fn validate_series_metadata_links(
    links: &[SeriesMetadataLinkRecord],
) -> Result<(), SeriesMetadataUpdateError> {
    for link in links {
        if link.label.trim().is_empty() {
            return Err(SeriesMetadataUpdateError::validation(
                "links.label must not be blank",
            ));
        }
        if Url::parse(&link.url).is_err() {
            return Err(SeriesMetadataUpdateError::validation(
                "links.url must be a valid URL",
            ));
        }
    }

    Ok(())
}

fn validate_series_alternate_titles(
    alternate_titles: &[SeriesAlternateTitleRecord],
) -> Result<(), SeriesMetadataUpdateError> {
    for alternate_title in alternate_titles {
        if alternate_title.label.trim().is_empty() {
            return Err(SeriesMetadataUpdateError::validation(
                "alternateTitles.label must not be blank",
            ));
        }
        if alternate_title.title.trim().is_empty() {
            return Err(SeriesMetadataUpdateError::validation(
                "alternateTitles.title must not be blank",
            ));
        }
    }

    Ok(())
}

pub fn apply_series_metadata_patch(
    existing: &ExistingSeriesMetadataRecord,
    patch: SeriesMetadataPatch,
) -> SeriesMetadataUpdateRecord {
    SeriesMetadataUpdateRecord {
        status: patch.status.unwrap_or(existing.status),
        status_lock: patch.status_lock.unwrap_or(existing.status_lock),
        title: patch.title.unwrap_or_else(|| existing.title.clone()),
        title_lock: patch.title_lock.unwrap_or(existing.title_lock),
        title_sort: patch
            .title_sort
            .unwrap_or_else(|| existing.title_sort.clone()),
        title_sort_lock: patch.title_sort_lock.unwrap_or(existing.title_sort_lock),
        summary: patch.summary.unwrap_or_else(|| existing.summary.clone()),
        summary_lock: patch.summary_lock.unwrap_or(existing.summary_lock),
        reading_direction: patch
            .reading_direction
            .unwrap_or(existing.reading_direction),
        reading_direction_lock: patch
            .reading_direction_lock
            .unwrap_or(existing.reading_direction_lock),
        publisher: patch
            .publisher
            .unwrap_or_else(|| existing.publisher.clone()),
        publisher_lock: patch.publisher_lock.unwrap_or(existing.publisher_lock),
        age_rating: patch.age_rating.unwrap_or(existing.age_rating),
        age_rating_lock: patch.age_rating_lock.unwrap_or(existing.age_rating_lock),
        language: patch.language.unwrap_or_else(|| existing.language.clone()),
        language_lock: patch.language_lock.unwrap_or(existing.language_lock),
        genres: patch.genres.unwrap_or_else(|| existing.genres.clone()),
        genres_lock: patch.genres_lock.unwrap_or(existing.genres_lock),
        tags: patch.tags.unwrap_or_else(|| existing.tags.clone()),
        tags_lock: patch.tags_lock.unwrap_or(existing.tags_lock),
        total_book_count: patch.total_book_count.unwrap_or(existing.total_book_count),
        total_book_count_lock: patch
            .total_book_count_lock
            .unwrap_or(existing.total_book_count_lock),
        sharing_labels: patch
            .sharing_labels
            .unwrap_or_else(|| existing.sharing_labels.clone()),
        sharing_labels_lock: patch
            .sharing_labels_lock
            .unwrap_or(existing.sharing_labels_lock),
        links: patch.links.unwrap_or_else(|| existing.links.clone()),
        links_lock: patch.links_lock.unwrap_or(existing.links_lock),
        alternate_titles: patch
            .alternate_titles
            .unwrap_or_else(|| existing.alternate_titles.clone()),
        alternate_titles_lock: patch
            .alternate_titles_lock
            .unwrap_or(existing.alternate_titles_lock),
    }
}
