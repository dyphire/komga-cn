use async_trait::async_trait;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::identity_access::{AuthUser, user_id};

use super::book_access::BookAccessContext;
use super::{
    BookMediaPort, BookMediaRecord, BookProgressionInput, ContentAccessPort, ContentResolverPort,
    EpubNavigationError, EpubNavigationLoadError, EpubNavigationReaderPort, ProgressWriterPort,
    ReadProgressReadPort, book_media_is_epub, load_book_epub_navigation,
};

pub struct BookProgressionService<'a, R, C, W>
where
    R: BookProgressionReaderPort + EpubNavigationReaderPort + ?Sized,
    C: ContentResolverPort + ?Sized,
    W: ProgressWriterPort + ?Sized,
{
    reader: &'a R,
    content: &'a C,
    writer: &'a W,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookProgressionUpdate {
    pub modified: String,
    pub device_id: String,
    pub device_name: String,
    pub locator: Option<Value>,
}

impl BookProgressionUpdate {
    pub fn from_payload(payload: &Value) -> Option<Self> {
        let modified = payload.get("modified").and_then(Value::as_str)?.to_string();
        let device_id = payload
            .get("device")
            .and_then(|value| value.get("id"))
            .and_then(Value::as_str)?
            .to_string();
        let device_name = payload
            .get("device")
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)?
            .to_string();
        Some(Self {
            modified,
            device_id,
            device_name,
            locator: payload.get("locator").cloned(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BookProgressionOutcome {
    Updated,
    NotFound,
    Forbidden,
    InvalidPayload,
    BadRequest(String),
    Conflict,
    Internal(String),
}

#[derive(Clone, Debug, PartialEq)]
pub enum BookProgressionGetOutcome {
    Progression(Value),
    NoContent,
    NotFound,
    Forbidden,
    Internal(String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResolvedBookProgression {
    pub page: u64,
    pub completed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BookProgressionPageSource {
    TotalProgression,
    LocatorPosition,
}

pub fn resolve_book_progression_write(
    page_count: u64,
    progression: f64,
    page_source: BookProgressionPageSource,
    locator: Option<&Value>,
) -> ResolvedBookProgression {
    let page_count = page_count.max(1);
    let effective_progression = locator
        .and_then(locator_total_progression)
        .unwrap_or(progression);
    let page_from_progression = (effective_progression * page_count as f64)
        .round()
        .clamp(0.0, page_count as f64) as u64;
    let page = match page_source {
        BookProgressionPageSource::LocatorPosition => locator
            .and_then(locator_position)
            .filter(|value| *value >= 1)
            .unwrap_or(page_from_progression),
        BookProgressionPageSource::TotalProgression => page_from_progression,
    };

    ResolvedBookProgression {
        page,
        completed: effective_progression >= 0.99,
    }
}

#[async_trait]
pub trait BookProgressionReaderPort: Send + Sync {
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String>;

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String>;

    async fn book_progression(&self, book_id: &str, user_id: &str)
    -> Result<Option<Value>, String>;
}

#[async_trait]
impl<T> BookProgressionReaderPort for T
where
    T: BookMediaPort + ContentAccessPort + ReadProgressReadPort + ?Sized,
{
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        BookMediaPort::book_media(self, book_id).await
    }

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<(Option<u16>, Vec<String>)>, String> {
        ContentAccessPort::book_restrictions(self, book_id).await
    }

    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<Value>, String> {
        ReadProgressReadPort::book_progression(self, book_id, user_id).await
    }
}

impl<'a, R, C, W> BookProgressionService<'a, R, C, W>
where
    R: BookProgressionReaderPort + EpubNavigationReaderPort + ?Sized,
    C: ContentResolverPort + ?Sized,
    W: ProgressWriterPort + ?Sized,
{
    pub fn new(reader: &'a R, content: &'a C, writer: &'a W) -> Self {
        Self {
            reader,
            content,
            writer,
        }
    }

    pub async fn update_progression(
        &self,
        user: &AuthUser,
        book_id: &str,
        update: BookProgressionUpdate,
    ) -> BookProgressionOutcome {
        let media = match self.accessible_book_media(user, book_id).await {
            Ok(media) => media,
            Err(outcome) => return outcome,
        };

        let is_epub = book_media_is_epub(&media);
        let page_count = media.page_count.max(1);
        let locator = update.locator.as_ref();
        let (progression, page_source, locator_to_persist) = if is_epub {
            let Some(locator) = locator else {
                return BookProgressionOutcome::InvalidPayload;
            };
            let navigation =
                match load_book_epub_navigation(self.reader, self.content, book_id).await {
                    Ok(navigation) => navigation,
                    Err(error) => return book_progression_outcome_from_epub_load_error(error),
                };
            let normalized_locator = match navigation.normalize_locator(locator) {
                Ok(locator) => locator,
                Err(error) => return book_progression_outcome_from_epub_error(error),
            };
            let Some(progression) = locator_progression(&normalized_locator) else {
                return BookProgressionOutcome::InvalidPayload;
            };
            if !(0.0..=1.0).contains(&progression) {
                return BookProgressionOutcome::InvalidPayload;
            }
            (
                progression,
                BookProgressionPageSource::TotalProgression,
                Some(normalized_locator),
            )
        } else {
            let Some(position) = locator.and_then(locator_position) else {
                return BookProgressionOutcome::InvalidPayload;
            };
            if !(1..=page_count).contains(&position) {
                return BookProgressionOutcome::BadRequest(format!(
                    "Page argument ({position}) must be within 1 and book page count ({page_count})"
                ));
            }
            (
                position as f64 / page_count as f64,
                BookProgressionPageSource::LocatorPosition,
                update.locator.clone(),
            )
        };

        match progression_is_older_than_existing(
            self.reader,
            book_id,
            user_id(user),
            &update.modified,
        )
        .await
        {
            Ok(true) => return BookProgressionOutcome::Conflict,
            Ok(false) => {}
            Err(error) => return BookProgressionOutcome::Internal(error),
        }

        let resolved = resolve_book_progression_write(
            page_count,
            progression,
            page_source,
            locator_to_persist.as_ref(),
        );

        match self
            .writer
            .persist_book_progression(BookProgressionInput {
                book_id: book_id.to_string(),
                user_id: user_id(user).to_string(),
                page: resolved.page,
                completed: resolved.completed,
                modified: Some(update.modified),
                device_id: Some(update.device_id),
                device_name: Some(update.device_name),
                locator: locator_to_persist,
            })
            .await
        {
            Ok(()) => BookProgressionOutcome::Updated,
            Err(error) => BookProgressionOutcome::Internal(error),
        }
    }

    pub async fn progression(&self, user: &AuthUser, book_id: &str) -> BookProgressionGetOutcome {
        let media = match self.accessible_book_media(user, book_id).await {
            Ok(media) => media,
            Err(BookProgressionOutcome::NotFound) => return BookProgressionGetOutcome::NotFound,
            Err(BookProgressionOutcome::Forbidden) => return BookProgressionGetOutcome::Forbidden,
            Err(BookProgressionOutcome::Internal(error)) => {
                return BookProgressionGetOutcome::Internal(error);
            }
            Err(_) => return BookProgressionGetOutcome::NotFound,
        };
        drop(media);

        match self.reader.book_progression(book_id, user_id(user)).await {
            Ok(Some(progression)) => BookProgressionGetOutcome::Progression(progression),
            Ok(None) => BookProgressionGetOutcome::NoContent,
            Err(error) => BookProgressionGetOutcome::Internal(error),
        }
    }

    async fn accessible_book_media(
        &self,
        user: &AuthUser,
        book_id: &str,
    ) -> Result<BookMediaRecord, BookProgressionOutcome> {
        let media = match self.reader.book_media(book_id).await {
            Ok(Some(media)) => media,
            Ok(None) => return Err(BookProgressionOutcome::NotFound),
            Err(error) => return Err(BookProgressionOutcome::Internal(error)),
        };
        if !self.user_can_access_book(book_id, user, &media).await {
            return Err(BookProgressionOutcome::Forbidden);
        }

        Ok(media)
    }

    async fn user_can_access_book(
        &self,
        book_id: &str,
        user: &AuthUser,
        media: &BookMediaRecord,
    ) -> bool {
        let context = BookAccessContext::from_auth_user(user);
        if !context.can_access_library(&media.library_id) {
            return false;
        }

        let Ok(Some((age_rating, labels))) = self.reader.book_restrictions(book_id).await else {
            return true;
        };

        context.content_allowed(age_rating, &labels)
    }
}

pub async fn progression_is_older_than_existing<R>(
    reader: &R,
    book_id: &str,
    user_id: &str,
    modified: &str,
) -> Result<bool, String>
where
    R: BookProgressionReaderPort + ?Sized,
{
    let Ok(new_modified) = OffsetDateTime::parse(modified, &Rfc3339) else {
        return Ok(false);
    };
    let Some(existing_progression) = reader.book_progression(book_id, user_id).await? else {
        return Ok(false);
    };
    let Some(existing_modified) = existing_progression.get("modified").and_then(Value::as_str)
    else {
        return Ok(false);
    };
    let Ok(existing_modified) = OffsetDateTime::parse(existing_modified, &Rfc3339) else {
        return Ok(false);
    };

    Ok(new_modified <= existing_modified)
}

pub fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

pub fn locator_position(locator: &Value) -> Option<u64> {
    locator
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_u64)
}

fn locator_total_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
}

fn book_progression_outcome_from_epub_error(error: EpubNavigationError) -> BookProgressionOutcome {
    match error {
        EpubNavigationError::BadRequest(error) => BookProgressionOutcome::BadRequest(error),
        EpubNavigationError::Internal(error) => BookProgressionOutcome::Internal(error),
    }
}

fn book_progression_outcome_from_epub_load_error(
    error: EpubNavigationLoadError,
) -> BookProgressionOutcome {
    match error {
        EpubNavigationLoadError::MissingExtension => {
            BookProgressionOutcome::BadRequest("Epub extension not found".to_string())
        }
        EpubNavigationLoadError::Internal(error) => BookProgressionOutcome::Internal(error),
    }
}
