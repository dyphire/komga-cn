use async_trait::async_trait;
use serde_json::Value;

use crate::identity_access::{AuthUser, user_id};

use super::book_access::BookAccessContext;
use super::book_progression_write::{
    BookProgressionConflictPolicy, BookProgressionWrite, BookProgressionWriteError,
    BookProgressionWriteService, BookProgressionWriteSource,
};
use super::{
    BookMediaPort, BookMediaRecord, ContentAccessPort, ContentResolverPort, EpubNavigationError,
    EpubNavigationLoadError, EpubNavigationReaderPort, ProgressWriterPort, ReadProgressReadPort,
    book_media_is_epub, load_book_epub_navigation, normalized_href_base,
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
struct BookProgressionUpdate {
    modified: String,
    device_id: String,
    device_name: String,
    locator: Option<Value>,
}

impl BookProgressionUpdate {
    fn from_payload(payload: &Value) -> Option<Self> {
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
        payload: &Value,
    ) -> BookProgressionOutcome {
        let media = match self.accessible_book_media(user, book_id).await {
            Ok(media) => media,
            Err(outcome) => return outcome,
        };
        let Some(update) = BookProgressionUpdate::from_payload(payload) else {
            return BookProgressionOutcome::InvalidPayload;
        };

        let is_epub = book_media_is_epub(&media);
        let page_count = media.page_count.max(1);
        let locator = update.locator.as_ref();
        let source = if is_epub {
            let Some(locator) = locator else {
                return BookProgressionOutcome::InvalidPayload;
            };
            if let Err(error) = validate_epub_locator_payload(locator) {
                return book_progression_outcome_from_epub_error(error);
            }
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

            BookProgressionWriteSource::TotalProgression {
                progression,
                locator: Some(normalized_locator),
            }
        } else {
            let Some(position) = locator.and_then(locator_position) else {
                return BookProgressionOutcome::InvalidPayload;
            };
            if !(1..=page_count).contains(&position) {
                return BookProgressionOutcome::BadRequest(format!(
                    "Page argument ({position}) must be within 1 and book page count ({page_count})"
                ));
            }
            BookProgressionWriteSource::Position {
                progression: position as f64 / page_count as f64,
                locator: update.locator.clone(),
            }
        };

        let writer = BookProgressionWriteService::new(self.reader, self.writer);
        match writer
            .persist(BookProgressionWrite {
                book_id: book_id.to_string(),
                user_id: user_id(user).to_string(),
                page_count,
                source,
                modified: Some(update.modified),
                device_id: Some(update.device_id),
                device_name: Some(update.device_name),
                conflict_policy: BookProgressionConflictPolicy::RejectStale,
            })
            .await
        {
            Ok(()) => BookProgressionOutcome::Updated,
            Err(BookProgressionWriteError::Stale) => BookProgressionOutcome::Conflict,
            Err(BookProgressionWriteError::Internal(error)) => {
                BookProgressionOutcome::Internal(error)
            }
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

pub(super) fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

pub(super) fn locator_position(locator: &Value) -> Option<u64> {
    locator
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_u64)
}

fn book_progression_outcome_from_epub_error(error: EpubNavigationError) -> BookProgressionOutcome {
    match error {
        EpubNavigationError::BadRequest(error) => BookProgressionOutcome::BadRequest(error),
        EpubNavigationError::Internal(error) => BookProgressionOutcome::Internal(error),
    }
}

fn validate_epub_locator_payload(locator: &Value) -> Result<(), EpubNavigationError> {
    let href_base = normalized_href_base(
        locator
            .get("href")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if href_base.is_empty() {
        return Err(EpubNavigationError::BadRequest(
            "Resource does not exist in book: ".to_string(),
        ));
    }

    if locator_progression(locator).is_none() {
        return Err(EpubNavigationError::BadRequest(
            "location.progression is required".to_string(),
        ));
    }

    Ok(())
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
