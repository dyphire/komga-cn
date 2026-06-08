use serde_json::{Value, json};

use crate::media_assets::{
    BookProgressionConflictPolicy, BookProgressionWrite, BookProgressionWriteError,
    BookProgressionWriteService, BookProgressionWriteSource, ContentResolverPort, EpubNavigation,
    EpubNavigationError, EpubNavigationLoadError, MediaReaderPort, ProgressWriterPort,
    load_book_epub_navigation,
};

use super::DeviceSyncPort;

pub struct DeviceProgressService<'a> {
    device_sync: &'a dyn DeviceSyncPort,
    reader: &'a dyn MediaReaderPort,
    content: &'a dyn ContentResolverPort,
    progress: &'a dyn ProgressWriterPort,
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

pub struct KoboReadingStateUpdate {
    pub last_modified: String,
    pub status: String,
    pub progress_percent: Option<f64>,
    pub content_source_progress_percent: Option<f64>,
    pub location_source: String,
    pub location_type: String,
    pub location_value: Option<String>,
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

impl<'a> DeviceProgressService<'a> {
    pub fn new(
        device_sync: &'a dyn DeviceSyncPort,
        reader: &'a dyn MediaReaderPort,
        content: &'a dyn ContentResolverPort,
        progress: &'a dyn ProgressWriterPort,
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
    ) -> Result<Value, DeviceProgressError> {
        let progress = self
            .device_sync
            .load_read_progress(book_id, user_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?;

        Ok(match progress {
            Some(record) => kobo_reading_state_payload(
                book_id,
                &record,
                parse_locator_payload(record.locator.as_deref()),
            ),
            None => kobo_empty_reading_state_payload(book_id, fallback_created_timestamp),
        })
    }

    pub async fn update_kobo_reading_state(
        &self,
        book_id: &str,
        user_id: &str,
        update: KoboReadingStateUpdate,
    ) -> Result<(), DeviceProgressError> {
        let completed = update.status.eq_ignore_ascii_case("Finished");
        let content_source_progress = update.content_source_progress_percent.ok_or_else(|| {
            DeviceProgressError::BadRequest("ContentSourceProgressPercent is required".to_string())
        })? / 100.0;
        let total_progress = update.progress_percent.map(|value| value / 100.0);

        let locator = if completed {
            self.device_sync
                .load_book_last_epub_position_locator(book_id)
                .await
                .map_err(|_| DeviceProgressError::Persistence)?
                .ok_or(DeviceProgressError::Persistence)?
        } else {
            let request_locator = json!({
                "href": update.location_source,
                "type": "application/xhtml+xml",
                "koboSpan": if update.location_type.eq_ignore_ascii_case("kobospan") {
                    update.location_value.clone()
                } else {
                    None
                },
                "locations": {
                    "progression": content_source_progress,
                    "totalProgression": total_progress,
                },
            });

            self.book_epub_navigation(book_id)
                .await?
                .normalize_locator(&request_locator)
                .map_err(device_progress_error_from_epub_error)?
        };

        let locator_progression = locator
            .get("locations")
            .and_then(|value| value.get("progression"))
            .and_then(Value::as_f64)
            .unwrap_or(if completed {
                1.0
            } else {
                content_source_progress
            });
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

        let locator = parse_locator_payload(progress.locator.as_deref());
        let percentage = locator
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .and_then(Value::as_f64)
            .unwrap_or_else(|| {
                (progress.page.max(0) as f64 / target.page_count.max(1) as f64).clamp(0.0, 1.0)
            });
        let progress_value = match self
            .koreader_epub_progress_value(&target.id, &locator)
            .await
        {
            Some(progress_value) => progress_value,
            None => locator
                .get("koreaderProgress")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| progress.page.max(0).to_string()),
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

    async fn koreader_epub_progress_value(&self, book_id: &str, locator: &Value) -> Option<String> {
        self.book_epub_navigation(book_id)
            .await
            .ok()?
            .koreader_progress_for_locator(locator)
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
        "application/epub+zip" => Some(KoreaderMediaProfile::Epub),
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

fn parse_locator_payload(locator: Option<&[u8]>) -> Value {
    locator
        .and_then(|blob| serde_json::from_slice::<Value>(blob).ok())
        .unwrap_or_else(|| json!({}))
}

fn kobo_empty_reading_state_payload(book_id: &str, created_timestamp: &str) -> Value {
    json!({
        "Created": created_timestamp,
        "CurrentBookmark": {
            "LastModified": created_timestamp,
        },
        "EntitlementId": book_id,
        "LastModified": created_timestamp,
        "PriorityTimestamp": created_timestamp,
        "Statistics": {
            "LastModified": created_timestamp,
        },
        "StatusInfo": {
            "LastModified": created_timestamp,
            "Status": "ReadyToRead",
            "TimesStartedReading": 0,
        },
    })
}

fn kobo_reading_state_payload(
    book_id: &str,
    progress: &super::PersistedReadProgressRecord,
    locator: Value,
) -> Value {
    let source = locator.get("href").and_then(Value::as_str);
    let kobo_span = locator.get("koboSpan").and_then(Value::as_str);
    let mut current_bookmark = json!({
        "LastModified": progress.last_modified,
    });
    let current_bookmark_object = current_bookmark
        .as_object_mut()
        .expect("reading state bookmark should be an object");

    if let Some(total_progression) = locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
    {
        current_bookmark_object.insert(
            "ProgressPercent".to_string(),
            json!(total_progression * 100.0),
        );
    }
    if let Some(source_progression) = locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
    {
        current_bookmark_object.insert(
            "ContentSourceProgressPercent".to_string(),
            json!(source_progression * 100.0),
        );
    }

    if source.is_some() || kobo_span.is_some() {
        let mut location = json!({
            "Source": source.unwrap_or_default(),
            "Type": "KoboSpan",
        });
        if let Some(kobo_span) = kobo_span {
            location
                .as_object_mut()
                .expect("reading state location should be an object")
                .insert("Value".to_string(), Value::String(kobo_span.to_string()));
        }
        current_bookmark_object.insert("Location".to_string(), location);
    }

    json!({
        "Created": progress.created,
        "CurrentBookmark": current_bookmark,
        "EntitlementId": book_id,
        "LastModified": progress.last_modified,
        "PriorityTimestamp": progress.last_modified,
        "Statistics": {
            "LastModified": progress.last_modified,
        },
        "StatusInfo": {
            "LastModified": progress.last_modified,
            "Status": if progress.completed { "Finished" } else { "Reading" },
            "TimesStartedReading": 1,
        },
    })
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
