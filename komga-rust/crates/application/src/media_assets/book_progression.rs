use serde_json::Value;

use crate::identity_access::{AuthUser, user_id};

use super::book_access::BookAccessContext;
use super::book_progression_write::{
    BookProgressionConflictPolicy, BookProgressionWrite, BookProgressionWriteError,
    BookProgressionWriteService, BookProgressionWriteSource, BookProgressionWriterPort,
};
use super::{
    BookAccessRestrictions, BookMediaPort, BookMediaRecord, BookProgressionRecord,
    ContentAccessPort, EpubNavigationContentPort, EpubNavigationError, EpubNavigationLoadError,
    EpubNavigationReaderPort, ReadProgressReadPort, book_media_is_epub, load_book_epub_navigation,
    normalized_href_base,
};

pub struct BookProgressionService<'a, R, C, W>
where
    R: BookProgressionReaderPort + EpubNavigationReaderPort + ?Sized,
    C: EpubNavigationContentPort + ?Sized,
    W: BookProgressionWriterPort + ?Sized,
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
    pub locator: Option<BookProgressionLocator>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BookProgressionLocator {
    raw: Value,
    href: Option<String>,
    progression: Option<f64>,
    position: Option<u64>,
    total_progression: Option<f64>,
}

impl BookProgressionLocator {
    pub fn new(
        raw: Value,
        href: Option<String>,
        progression: Option<f64>,
        position: Option<u64>,
        total_progression: Option<f64>,
    ) -> Self {
        Self {
            raw,
            href,
            progression,
            position,
            total_progression,
        }
    }

    pub fn raw(&self) -> &Value {
        &self.raw
    }

    pub fn into_raw(self) -> Value {
        self.raw
    }

    fn href_base(&self) -> String {
        normalized_href_base(self.href.as_deref().unwrap_or_default())
    }

    fn progression(&self) -> Option<f64> {
        self.progression
    }

    fn position(&self) -> Option<u64> {
        self.position
    }

    fn total_progression(&self) -> Option<f64> {
        self.total_progression
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BookProgressionUpdateInput {
    Update(BookProgressionUpdate),
    InvalidPayload,
}

impl From<BookProgressionUpdate> for BookProgressionUpdateInput {
    fn from(update: BookProgressionUpdate) -> Self {
        Self::Update(update)
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
    Progression(BookProgressionRecord),
    NoContent,
    NotFound,
    Forbidden,
    Internal(String),
}

pub trait BookProgressionSurfacePort:
    BookProgressionReaderPort + EpubNavigationReaderPort + Send + Sync
{
}

impl<T> BookProgressionSurfacePort for T where
    T: BookProgressionReaderPort + EpubNavigationReaderPort + Send + Sync + ?Sized
{
}

#[async_trait::async_trait]
pub trait BookProgressionReaderPort: Send + Sync {
    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String>;

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<BookAccessRestrictions>, String>;

    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<BookProgressionRecord>, String>;
}

#[async_trait::async_trait]
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
    ) -> Result<Option<BookAccessRestrictions>, String> {
        ContentAccessPort::book_restrictions(self, book_id).await
    }

    async fn book_progression(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<BookProgressionRecord>, String> {
        ReadProgressReadPort::book_progression(self, book_id, user_id).await
    }
}

impl<'a, R, C, W> BookProgressionService<'a, R, C, W>
where
    R: BookProgressionReaderPort + EpubNavigationReaderPort + ?Sized,
    C: EpubNavigationContentPort + ?Sized,
    W: BookProgressionWriterPort + ?Sized,
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
        input: impl Into<BookProgressionUpdateInput>,
    ) -> BookProgressionOutcome {
        let media = match self.accessible_book_media(user, book_id).await {
            Ok(media) => media,
            Err(outcome) => return outcome,
        };
        let update = match input.into() {
            BookProgressionUpdateInput::Update(update) => update,
            BookProgressionUpdateInput::InvalidPayload => {
                return BookProgressionOutcome::InvalidPayload;
            }
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
            let normalized_locator = match navigation.normalize_locator(locator.raw()) {
                Ok(locator) => locator,
                Err(error) => return book_progression_outcome_from_epub_error(error),
            };
            let progression = normalized_locator.progression();
            if !(0.0..=1.0).contains(&progression) {
                return BookProgressionOutcome::InvalidPayload;
            }

            BookProgressionWriteSource::TotalProgression {
                progression,
                total_progression: normalized_locator.total_progression(),
                locator: Some(normalized_locator.into_raw()),
            }
        } else {
            let Some(position) = locator.and_then(BookProgressionLocator::position) else {
                return BookProgressionOutcome::InvalidPayload;
            };
            if !(1..=page_count).contains(&position) {
                return BookProgressionOutcome::BadRequest(format!(
                    "Page argument ({position}) must be within 1 and book page count ({page_count})"
                ));
            }
            BookProgressionWriteSource::Position {
                progression: position as f64 / page_count as f64,
                position,
                total_progression: locator.and_then(BookProgressionLocator::total_progression),
                locator: update.locator.map(BookProgressionLocator::into_raw),
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
        match self.user_can_access_book(book_id, user, &media).await {
            Ok(true) => {}
            Ok(false) => return Err(BookProgressionOutcome::Forbidden),
            Err(error) => return Err(BookProgressionOutcome::Internal(error)),
        }

        Ok(media)
    }

    async fn user_can_access_book(
        &self,
        book_id: &str,
        user: &AuthUser,
        media: &BookMediaRecord,
    ) -> Result<bool, String> {
        let context = BookAccessContext::from_auth_user(user);
        if !context.can_access_library(&media.library_id) {
            return Ok(false);
        }

        let Some(restrictions) = self.reader.book_restrictions(book_id).await? else {
            return Ok(true);
        };

        Ok(context.content_allowed(restrictions.age_rating, &restrictions.labels))
    }
}

fn book_progression_outcome_from_epub_error(error: EpubNavigationError) -> BookProgressionOutcome {
    match error {
        EpubNavigationError::BadRequest(error) => BookProgressionOutcome::BadRequest(error),
        EpubNavigationError::Internal(error) => BookProgressionOutcome::Internal(error),
    }
}

fn validate_epub_locator_payload(
    locator: &BookProgressionLocator,
) -> Result<(), EpubNavigationError> {
    let href_base = locator.href_base();
    if href_base.is_empty() {
        return Err(EpubNavigationError::BadRequest(
            "Resource does not exist in book: ".to_string(),
        ));
    }

    if locator.progression().is_none() {
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
