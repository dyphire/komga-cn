use serde_json::{Value, json};

use crate::media_assets::{
    BookProgressionConflictPolicy, BookProgressionWrite, BookProgressionWriteError,
    BookProgressionWriteReaderPort, BookProgressionWriteService, BookProgressionWriteSource,
    BookProgressionWriterPort, EpubNavigation, EpubNavigationContentPort, EpubNavigationError,
    EpubNavigationLoadError, EpubNavigationReaderPort, ReadProgressReadPort,
    load_book_epub_navigation,
};

use super::DeviceSyncPort;

#[async_trait::async_trait]
pub trait DeviceProgressPageCountPort: Send + Sync {
    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>>;
}

#[async_trait::async_trait]
impl<T> DeviceProgressPageCountPort for T
where
    T: ReadProgressReadPort + ?Sized,
{
    async fn book_page_count(&self, book_id: &str) -> anyhow::Result<Option<u64>> {
        ReadProgressReadPort::book_page_count(self, book_id).await
    }
}

pub trait DeviceProgressReaderPort:
    BookProgressionWriteReaderPort
    + EpubNavigationReaderPort
    + DeviceProgressPageCountPort
    + Send
    + Sync
{
}

impl<T> DeviceProgressReaderPort for T where
    T: BookProgressionWriteReaderPort
        + EpubNavigationReaderPort
        + DeviceProgressPageCountPort
        + Send
        + Sync
        + ?Sized
{
}

pub struct DeviceProgressService<'a, C: ?Sized, W: ?Sized> {
    device_sync: &'a dyn DeviceSyncPort,
    reader: &'a dyn DeviceProgressReaderPort,
    content: &'a C,
    progress: &'a W,
}

pub struct KoreaderProgressUpdate {
    pub document: String,
    pub percentage: f64,
    pub progress: String,
    pub device: String,
    pub device_id: String,
    pub modified: String,
}

pub struct KoreaderProgressSnapshot {
    pub percentage: f64,
    pub progress: String,
    pub device: String,
    pub device_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct KoboReadingStateSnapshot {
    pub book_id: String,
    pub created: String,
    pub last_modified: String,
    pub status: KoboReadingStateStatus,
    pub times_started_reading: u64,
    pub total_progress_percent: Option<f64>,
    pub content_source_progress_percent: Option<f64>,
    pub location: Option<KoboReadingStateLocationSnapshot>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KoboReadingStateStatus {
    ReadyToRead,
    Reading,
    Finished,
}

impl KoboReadingStateStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadyToRead => "ReadyToRead",
            Self::Reading => "Reading",
            Self::Finished => "Finished",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboReadingStateLocationSnapshot {
    pub source: String,
    pub kobo_span: Option<String>,
}

pub struct KoboReadingStateUpdate {
    pub last_modified: String,
    pub status: KoboReadingStateStatus,
    pub progress_percent: Option<f64>,
    pub content_source_progress_percent: Option<f64>,
    pub location_source: String,
    pub kobo_span: Option<String>,
    pub device_id: String,
    pub device_name: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeviceProgressError {
    NotFound,
    NoProgress,
    Conflict,
    BadRequest(String),
    UnsupportedMediaProfile,
    Persistence,
}

enum KoreaderMediaProfile {
    Visual,
    Epub,
}

impl<'a, C, W> DeviceProgressService<'a, C, W>
where
    C: EpubNavigationContentPort + ?Sized,
    W: BookProgressionWriterPort + ?Sized,
{
    pub fn new(
        device_sync: &'a dyn DeviceSyncPort,
        reader: &'a dyn DeviceProgressReaderPort,
        content: &'a C,
        progress: &'a W,
    ) -> Self {
        Self {
            device_sync,
            reader,
            content,
            progress,
        }
    }

    pub async fn kobo_reading_state(
        &self,
        book_id: &str,
        user_id: &str,
        fallback_created_timestamp: &str,
    ) -> Result<KoboReadingStateSnapshot, DeviceProgressError> {
        let progress = self
            .device_sync
            .load_read_progress(book_id, user_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?;

        match progress {
            Some(record) => {
                let locator = parse_locator_payload(record.locator.as_deref())?;
                Ok(kobo_reading_state_snapshot(book_id, &record, locator))
            }
            None => Ok(kobo_empty_reading_state_snapshot(
                book_id,
                fallback_created_timestamp,
            )),
        }
    }

    pub async fn update_kobo_reading_state(
        &self,
        book_id: &str,
        user_id: &str,
        update: KoboReadingStateUpdate,
    ) -> Result<(), DeviceProgressError> {
        let content_source_progress = update.content_source_progress_percent.ok_or_else(|| {
            DeviceProgressError::BadRequest("ContentSourceProgressPercent is required".to_string())
        })? / 100.0;
        let total_progress = update.progress_percent.map(|value| value / 100.0);

        let (locator, locator_progression, locator_total_progression) =
            if update.status == KoboReadingStateStatus::Finished {
                let locator = self
                    .book_epub_navigation(book_id)
                    .await?
                    .positions()
                    .last()
                    .map(|position| position.raw().clone())
                    .ok_or(DeviceProgressError::Persistence)?;
                let locator_progression = locator_progression(&locator).unwrap_or(1.0);
                let locator_total_progression = locator_total_progression(&locator);
                (locator, locator_progression, locator_total_progression)
            } else {
                let request_locator = json!({
                    "href": update.location_source,
                    "type": "application/xhtml+xml",
                    "koboSpan": update.kobo_span.clone(),
                    "locations": {
                        "progression": content_source_progress,
                        "totalProgression": total_progress,
                    },
                });

                let normalized_locator = self
                    .book_epub_navigation(book_id)
                    .await?
                    .normalize_locator(&request_locator)
                    .map_err(device_progress_error_from_epub_error)?;
                let locator_progression = normalized_locator.progression();
                let locator_total_progression = normalized_locator.total_progression();
                (
                    normalized_locator.into_raw(),
                    locator_progression,
                    locator_total_progression,
                )
            };

        let page_count = self
            .reader
            .book_page_count(book_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?
            .unwrap_or(1)
            .max(1);

        let writer = BookProgressionWriteService::new(self.reader, self.progress);
        writer
            .persist(BookProgressionWrite {
                book_id: book_id.to_string(),
                user_id: user_id.to_string(),
                page_count,
                source: BookProgressionWriteSource::TotalProgression {
                    progression: locator_progression,
                    total_progression: locator_total_progression,
                    locator: Some(locator),
                },
                modified: Some(update.last_modified),
                device_id: Some(update.device_id),
                device_name: Some(update.device_name),
                conflict_policy: BookProgressionConflictPolicy::RejectStale,
            })
            .await
            .map_err(device_progress_error_from_write_error)
    }

    pub async fn update_koreader_progress(
        &self,
        user_id: &str,
        update: KoreaderProgressUpdate,
    ) -> Result<(), DeviceProgressError> {
        let target = self
            .device_sync
            .load_koreader_book_target(&update.document)
            .await
            .map_err(|error| match error {
                super::KoreaderBookLookupError::Conflict => DeviceProgressError::Conflict,
                super::KoreaderBookLookupError::Persistence => DeviceProgressError::Persistence,
            })?
            .ok_or(DeviceProgressError::NotFound)?;

        let source = match koreader_media_profile(&target.media_type) {
            Some(KoreaderMediaProfile::Visual) => {
                self.koreader_visual_progression(&target, &update.progress)?
            }
            Some(KoreaderMediaProfile::Epub) => {
                self.koreader_epub_progression(&target.id, &update.progress)
                    .await?
            }
            None => return Err(DeviceProgressError::UnsupportedMediaProfile),
        };

        let writer = BookProgressionWriteService::new(self.reader, self.progress);
        writer
            .persist(BookProgressionWrite {
                book_id: target.id,
                user_id: user_id.to_string(),
                page_count: target.page_count,
                source,
                modified: Some(update.modified),
                device_id: Some(update.device_id),
                device_name: Some(update.device),
                conflict_policy: BookProgressionConflictPolicy::Overwrite,
            })
            .await
            .map_err(device_progress_error_from_write_error)
    }

    pub async fn koreader_progress(
        &self,
        book_hash: &str,
        user_id: &str,
    ) -> Result<KoreaderProgressSnapshot, DeviceProgressError> {
        let target = self
            .device_sync
            .load_koreader_book_target(book_hash)
            .await
            .map_err(|error| match error {
                super::KoreaderBookLookupError::Conflict => DeviceProgressError::Conflict,
                super::KoreaderBookLookupError::Persistence => DeviceProgressError::Persistence,
            })?
            .ok_or(DeviceProgressError::NotFound)?;
        let progress = self
            .device_sync
            .load_read_progress(&target.id, user_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?
            .ok_or(DeviceProgressError::NoProgress)?;

        let locator = parse_locator_payload(progress.locator.as_deref())?;
        let percentage = locator
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .and_then(Value::as_f64)
            .unwrap_or_else(|| {
                (progress.page.max(0) as f64 / target.page_count.max(1) as f64).clamp(0.0, 1.0)
            });
        let fallback_progress_value = || {
            locator
                .get("koreaderProgress")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| progress.page.max(0).to_string())
        };
        let progress_value = match koreader_media_profile(&target.media_type) {
            Some(KoreaderMediaProfile::Epub) => self
                .koreader_epub_progress_value(&target.id, &locator)
                .await?
                .unwrap_or_else(fallback_progress_value),
            Some(KoreaderMediaProfile::Visual) => fallback_progress_value(),
            None => return Err(DeviceProgressError::UnsupportedMediaProfile),
        };

        Ok(KoreaderProgressSnapshot {
            percentage,
            progress: progress_value,
            device: progress.device_name,
            device_id: progress.device_id,
        })
    }

    fn koreader_visual_progression(
        &self,
        target: &super::KoreaderBookTarget,
        progress: &str,
    ) -> Result<BookProgressionWriteSource, DeviceProgressError> {
        let Some(page) = parse_koreader_progress_page(progress).map(|value| value as i64) else {
            return Err(DeviceProgressError::Persistence);
        };
        if !(1..=target.page_count.max(1) as i64).contains(&page) {
            return Err(DeviceProgressError::Persistence);
        }

        let progression = page as f64 / target.page_count.max(1) as f64;
        Ok(BookProgressionWriteSource::Position {
            progression,
            position: page as u64,
            total_progression: Some(progression),
            locator: Some(json!({
                "koreaderProgress": progress,
                "locations": {
                    "position": page,
                    "totalProgression": progression,
                },
            })),
        })
    }

    async fn koreader_epub_progression(
        &self,
        book_id: &str,
        progress: &str,
    ) -> Result<BookProgressionWriteSource, DeviceProgressError> {
        let locator = self
            .book_epub_navigation(book_id)
            .await?
            .koreader_locator_for_progress(progress)
            .map_err(device_progress_error_from_epub_error)?;

        Ok(BookProgressionWriteSource::TotalProgression {
            progression: 0.0,
            total_progression: locator_total_progression(&locator),
            locator: Some(locator),
        })
    }

    async fn book_epub_navigation(
        &self,
        book_id: &str,
    ) -> Result<EpubNavigation, DeviceProgressError> {
        load_book_epub_navigation(self.reader, self.content, book_id)
            .await
            .map_err(device_progress_error_from_epub_load_error)
    }

    async fn koreader_epub_progress_value(
        &self,
        book_id: &str,
        locator: &Value,
    ) -> Result<Option<String>, DeviceProgressError> {
        match load_book_epub_navigation(self.reader, self.content, book_id).await {
            Ok(navigation) => Ok(navigation.koreader_progress_for_locator(locator)),
            Err(EpubNavigationLoadError::MissingExtension) => Ok(None),
            Err(EpubNavigationLoadError::Internal(_)) => Err(DeviceProgressError::Persistence),
        }
    }
}

fn device_progress_error_from_write_error(error: BookProgressionWriteError) -> DeviceProgressError {
    match error {
        BookProgressionWriteError::Stale | BookProgressionWriteError::Internal(_) => {
            DeviceProgressError::Persistence
        }
    }
}

fn koreader_media_profile(media_type: &str) -> Option<KoreaderMediaProfile> {
    match media_type {
        "application/epub+zip" | "application/x-mobipocket-ebook" => {
            Some(KoreaderMediaProfile::Epub)
        }
        "application/pdf"
        | "application/zip"
        | "application/vnd.comicbook+zip"
        | "application/vnd.comicbook-rar"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => Some(KoreaderMediaProfile::Visual),
        value if value.starts_with("image/") => Some(KoreaderMediaProfile::Visual),
        _ => None,
    }
}

fn parse_koreader_progress_page(progress: &str) -> Option<u64> {
    progress.parse::<u64>().ok().filter(|value| *value > 0)
}

fn parse_locator_payload(locator: Option<&[u8]>) -> Result<Value, DeviceProgressError> {
    let Some(locator) = locator else {
        return Ok(json!({}));
    };

    let payload =
        serde_json::from_slice::<Value>(locator).map_err(|_| DeviceProgressError::Persistence)?;
    if payload.is_object() {
        Ok(payload)
    } else {
        Err(DeviceProgressError::Persistence)
    }
}

fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

fn locator_total_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
}

fn kobo_empty_reading_state_snapshot(
    book_id: &str,
    created_timestamp: &str,
) -> KoboReadingStateSnapshot {
    KoboReadingStateSnapshot {
        book_id: book_id.to_string(),
        created: created_timestamp.to_string(),
        last_modified: created_timestamp.to_string(),
        status: KoboReadingStateStatus::ReadyToRead,
        times_started_reading: 0,
        total_progress_percent: None,
        content_source_progress_percent: None,
        location: None,
    }
}

fn kobo_reading_state_snapshot(
    book_id: &str,
    progress: &super::PersistedReadProgressRecord,
    locator: Value,
) -> KoboReadingStateSnapshot {
    let source = locator.get("href").and_then(Value::as_str);
    let kobo_span = locator.get("koboSpan").and_then(Value::as_str);
    let total_progress_percent = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
        .map(|value| value * 100.0);
    let content_source_progress_percent = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
        .map(|value| value * 100.0);
    let location = if source.is_some() || kobo_span.is_some() {
        Some(KoboReadingStateLocationSnapshot {
            source: source.unwrap_or_default().to_string(),
            kobo_span: kobo_span.map(str::to_string),
        })
    } else {
        None
    };

    KoboReadingStateSnapshot {
        book_id: book_id.to_string(),
        created: progress.created.clone(),
        last_modified: progress.last_modified.clone(),
        status: if progress.completed {
            KoboReadingStateStatus::Finished
        } else {
            KoboReadingStateStatus::Reading
        },
        times_started_reading: 1,
        total_progress_percent,
        content_source_progress_percent,
        location,
    }
}

fn device_progress_error_from_epub_error(error: EpubNavigationError) -> DeviceProgressError {
    match error {
        EpubNavigationError::BadRequest(error) => DeviceProgressError::BadRequest(error),
        EpubNavigationError::Internal(_) => DeviceProgressError::Persistence,
    }
}

fn device_progress_error_from_epub_load_error(
    error: EpubNavigationLoadError,
) -> DeviceProgressError {
    match error {
        EpubNavigationLoadError::MissingExtension => {
            DeviceProgressError::BadRequest("Epub extension not found".to_string())
        }
        EpubNavigationLoadError::Internal(_) => DeviceProgressError::Persistence,
    }
}
