use async_trait::async_trait;
use serde_json::Value;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::identity_access::{AuthUser, user_id};

use super::book_access::BookAccessContext;
use super::{
    BookMediaPort, BookMediaRecord, BookProgressionInput, ContentAccessPort, ContentResolverPort,
    ProgressWriterPort, ReadProgressReadPort, book_media_is_epub,
};

pub struct BookProgressionService<'a, R, C, W>
where
    R: BookProgressionReaderPort + ?Sized,
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EpubProgressionError {
    BadRequest(String),
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

    async fn book_media_files(&self, book_id: &str) -> Result<Vec<String>, String>;

    async fn epub_extension_blob(&self, book_id: &str)
    -> Result<Option<(String, Vec<u8>)>, String>;
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

    async fn book_media_files(&self, book_id: &str) -> Result<Vec<String>, String> {
        BookMediaPort::book_media_files(self, book_id).await
    }

    async fn epub_extension_blob(
        &self,
        book_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String> {
        BookMediaPort::epub_extension_blob(self, book_id).await
    }
}

impl<'a, R, C, W> BookProgressionService<'a, R, C, W>
where
    R: BookProgressionReaderPort + ?Sized,
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
        let (progression, use_locator_position_for_page, locator_to_persist) = if is_epub {
            let Some(locator) = locator else {
                return BookProgressionOutcome::InvalidPayload;
            };
            let normalized_locator = match normalize_book_epub_locator(
                self.reader,
                self.content,
                book_id,
                locator,
            )
            .await
            {
                Ok(locator) => locator,
                Err(error) => return book_progression_outcome_from_epub_error(error),
            };
            let Some(progression) = locator_progression(&normalized_locator) else {
                return BookProgressionOutcome::InvalidPayload;
            };
            if !(0.0..=1.0).contains(&progression) {
                return BookProgressionOutcome::InvalidPayload;
            }
            (progression, false, Some(normalized_locator))
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
                true,
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

        match self
            .writer
            .persist_book_progression(BookProgressionInput {
                book_id: book_id.to_string(),
                user_id: user_id(user).to_string(),
                progression,
                use_locator_position_for_page,
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

pub async fn normalize_book_epub_locator<R, C>(
    reader: &R,
    content: &C,
    book_id: &str,
    locator: &Value,
) -> Result<Value, EpubProgressionError>
where
    R: BookProgressionReaderPort + ?Sized,
    C: ContentResolverPort + ?Sized,
{
    let href_base = normalized_href_base(
        locator
            .get("href")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    if href_base.is_empty() {
        return Err(EpubProgressionError::BadRequest(
            "Resource does not exist in book: ".to_string(),
        ));
    }

    let Some(locator_progression) = locator_progression(locator) else {
        return Err(EpubProgressionError::BadRequest(
            "location.progression is required".to_string(),
        ));
    };

    let persisted_media_files = reader
        .book_media_files(book_id)
        .await
        .map_err(EpubProgressionError::Internal)?;
    let persisted_resource_exists = (!persisted_media_files.is_empty()).then(|| {
        persisted_media_files
            .iter()
            .any(|file_name| normalized_href_base(file_name) == href_base)
    });
    if persisted_resource_exists == Some(false) {
        return Err(EpubProgressionError::BadRequest(format!(
            "Resource does not exist in book: {href_base}"
        )));
    }

    let Some((_class, blob)) = reader
        .epub_extension_blob(book_id)
        .await
        .map_err(EpubProgressionError::Internal)?
    else {
        return Err(EpubProgressionError::BadRequest(
            "Epub extension not found".to_string(),
        ));
    };
    let extension = content
        .decode_epub_positions_extension(&blob)
        .map_err(EpubProgressionError::Internal)?;

    if persisted_resource_exists.is_none()
        && !extension
            .positions
            .iter()
            .any(|position| position_matches_href(position, href_base.as_str()))
    {
        return Err(EpubProgressionError::BadRequest(format!(
            "Resource does not exist in book: {href_base}"
        )));
    }

    let Some(matched_position) = matched_epub_position(
        &extension.positions,
        href_base.as_str(),
        locator_progression,
        extension.is_fixed_layout,
    ) else {
        return Err(EpubProgressionError::BadRequest(
            "Invalid progression".to_string(),
        ));
    };

    Ok(normalized_epub_locator(locator, &matched_position))
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

pub fn normalized_href_base(href: &str) -> String {
    let base = href.split('#').next().unwrap_or(href).trim_end_matches('#');
    percent_decode(base).trim_start_matches('/').to_string()
}

fn book_progression_outcome_from_epub_error(error: EpubProgressionError) -> BookProgressionOutcome {
    match error {
        EpubProgressionError::BadRequest(error) => BookProgressionOutcome::BadRequest(error),
        EpubProgressionError::Internal(error) => BookProgressionOutcome::Internal(error),
    }
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1] as char;
            let lo = bytes[index + 2] as char;
            let parsed = hi
                .to_digit(16)
                .and_then(|hi| lo.to_digit(16).map(|lo| ((hi << 4) | lo) as u8));
            if let Some(byte) = parsed {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn position_progression(position: &Value) -> Option<f64> {
    position
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

fn position_number(position: &Value) -> Option<i64> {
    position
        .get("locations")
        .and_then(|value| value.get("position"))
        .and_then(Value::as_i64)
}

fn position_matches_href(position: &Value, href_base: &str) -> bool {
    position
        .get("href")
        .and_then(Value::as_str)
        .map(|value| normalized_href_base(value) == href_base)
        .unwrap_or(false)
}

fn matched_epub_position(
    positions: &[Value],
    href_base: &str,
    locator_progression: f64,
    is_fixed_layout: bool,
) -> Option<Value> {
    let matching_positions = positions
        .iter()
        .filter(|position| position_matches_href(position, href_base))
        .cloned()
        .collect::<Vec<_>>();

    matching_positions
        .iter()
        .find(|position| position_progression(position) == Some(locator_progression))
        .cloned()
        .or_else(|| {
            if is_fixed_layout && matching_positions.len() == 1 {
                return matching_positions.first().cloned();
            }

            let before = matching_positions
                .iter()
                .filter(|position| {
                    position_progression(position).is_some_and(|value| value < locator_progression)
                })
                .max_by_key(|position| position_number(position))
                .cloned();
            let after = matching_positions
                .iter()
                .filter(|position| {
                    position_progression(position).is_some_and(|value| value > locator_progression)
                })
                .min_by_key(|position| position_number(position))
                .cloned();

            match (before, after) {
                (Some(before), Some(_)) => Some(before),
                _ => None,
            }
        })
}

fn normalized_epub_locator(locator: &Value, matched_position: &Value) -> Value {
    let mut locator = locator.clone();
    let Some(locator_map) = locator.as_object_mut() else {
        return locator;
    };

    locator_map.insert(
        "type".to_string(),
        matched_position
            .get("type")
            .cloned()
            .unwrap_or_else(|| Value::String(String::new())),
    );

    let current_kobo_span_missing = locator_map.get("koboSpan").is_none_or(Value::is_null);
    if current_kobo_span_missing && let Some(kobo_span) = matched_position.get("koboSpan").cloned()
    {
        locator_map.insert("koboSpan".to_string(), kobo_span);
    }

    if let Some(locations) = locator_map
        .get_mut("locations")
        .and_then(Value::as_object_mut)
        && let Some(total_progression) = matched_position
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .cloned()
    {
        locations.insert("totalProgression".to_string(), total_progression);
    }

    locator
}
