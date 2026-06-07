use serde_json::{Value, json};
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;

use crate::media_assets::{
    BookProgressionInput, ContentResolverPort, MediaReaderPort, ProgressWriterPort,
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

            self.normalize_book_epub_locator(book_id, &request_locator)
                .await?
        };

        if self
            .progression_is_older_than_existing(book_id, user_id, &update.last_modified)
            .await?
        {
            return Err(DeviceProgressError::Persistence);
        }

        let locator_progression = locator
            .get("locations")
            .and_then(|value| value.get("progression"))
            .and_then(Value::as_f64)
            .unwrap_or(if completed {
                1.0
            } else {
                content_source_progress
            });

        self.progress
            .persist_book_progression(BookProgressionInput {
                book_id: book_id.to_string(),
                user_id: user_id.to_string(),
                progression: locator_progression,
                use_locator_position_for_page: false,
                modified: Some(update.last_modified),
                device_id: Some(update.device_id),
                device_name: Some(update.device_name),
                locator: Some(locator),
            })
            .await
            .map_err(|_| DeviceProgressError::Persistence)
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

        let (progression, use_locator_position_for_page, locator) =
            match koreader_media_profile(&target.media_type) {
                Some(KoreaderMediaProfile::Visual) => {
                    self.koreader_visual_progression(&target, &update.progress)?
                }
                Some(KoreaderMediaProfile::Epub) => {
                    self.koreader_epub_progression(&target.id, &update.progress)
                        .await?
                }
                None => return Err(DeviceProgressError::UnsupportedMediaProfile),
            };

        self.progress
            .persist_book_progression(BookProgressionInput {
                book_id: target.id,
                user_id: user_id.to_string(),
                progression,
                use_locator_position_for_page,
                modified: Some(update.modified),
                device_id: Some(update.device_id),
                device_name: Some(update.device),
                locator: Some(locator),
            })
            .await
            .map_err(|_| DeviceProgressError::Persistence)
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
    ) -> Result<(f64, bool, Value), DeviceProgressError> {
        let Some(page) = parse_koreader_progress_page(progress).map(|value| value as i64) else {
            return Err(DeviceProgressError::Persistence);
        };
        if !(1..=target.page_count.max(1) as i64).contains(&page) {
            return Err(DeviceProgressError::Persistence);
        }

        let progression = page as f64 / target.page_count.max(1) as f64;
        Ok((
            progression,
            true,
            json!({
                "koreaderProgress": progress,
                "locations": {
                    "position": page,
                    "totalProgression": progression,
                },
            }),
        ))
    }

    async fn koreader_epub_progression(
        &self,
        book_id: &str,
        progress: &str,
    ) -> Result<(f64, bool, Value), DeviceProgressError> {
        let (_extension_class, blob) = self
            .reader
            .epub_extension_blob(book_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?
            .ok_or_else(|| {
                DeviceProgressError::BadRequest("Epub extension not found".to_string())
            })?;
        let extension = self
            .content
            .decode_epub_positions_extension(&blob)
            .map_err(|_| DeviceProgressError::Persistence)?;
        let unique_hrefs = dedup_epub_hrefs(&extension.positions);
        let Some(resource_index) = parse_koreader_epub_resource_index(progress) else {
            return Err(DeviceProgressError::BadRequest(format!(
                "Could not get Epub resource index from progress: {progress}"
            )));
        };
        let Some(href) = unique_hrefs.get(resource_index) else {
            return Err(DeviceProgressError::Persistence);
        };
        let Some(matched_position) = extension
            .positions
            .iter()
            .find(|position| position.get("href").and_then(Value::as_str) == Some(href.as_str()))
        else {
            return Err(DeviceProgressError::BadRequest(format!(
                "Could not get Epub resource index from progress: {progress}"
            )));
        };

        Ok((0.0, false, koreader_epub_locator(href, matched_position)))
    }

    async fn normalize_book_epub_locator(
        &self,
        book_id: &str,
        locator: &Value,
    ) -> Result<Value, DeviceProgressError> {
        let href_base = normalized_href_base(
            locator
                .get("href")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        if href_base.is_empty() {
            return Err(DeviceProgressError::BadRequest(
                "Resource does not exist in book: ".to_string(),
            ));
        }

        let Some(locator_progression) = locator_progression(locator) else {
            return Err(DeviceProgressError::BadRequest(
                "location.progression is required".to_string(),
            ));
        };

        let persisted_media_files = self
            .reader
            .book_media_files(book_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?;
        let persisted_resource_exists = (!persisted_media_files.is_empty()).then(|| {
            persisted_media_files
                .iter()
                .any(|file_name| normalized_href_base(file_name) == href_base)
        });
        if persisted_resource_exists == Some(false) {
            return Err(DeviceProgressError::BadRequest(format!(
                "Resource does not exist in book: {href_base}"
            )));
        }

        let (_extension_class, blob) = self
            .reader
            .epub_extension_blob(book_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?
            .ok_or_else(|| {
                DeviceProgressError::BadRequest("Epub extension not found".to_string())
            })?;
        let extension = self
            .content
            .decode_epub_positions_extension(&blob)
            .map_err(|_| DeviceProgressError::Persistence)?;

        if persisted_resource_exists.is_none()
            && !extension
                .positions
                .iter()
                .any(|position| position_matches_href(position, href_base.as_str()))
        {
            return Err(DeviceProgressError::BadRequest(format!(
                "Resource does not exist in book: {href_base}"
            )));
        }

        let Some(matched_position) = matched_epub_position(
            &extension.positions,
            href_base.as_str(),
            locator_progression,
            extension.is_fixed_layout,
        ) else {
            return Err(DeviceProgressError::BadRequest(
                "Invalid progression".to_string(),
            ));
        };

        Ok(normalized_epub_locator(locator, &matched_position))
    }

    async fn progression_is_older_than_existing(
        &self,
        book_id: &str,
        user_id: &str,
        modified: &str,
    ) -> Result<bool, DeviceProgressError> {
        let Ok(new_modified) = OffsetDateTime::parse(modified, &Rfc3339) else {
            return Ok(false);
        };
        let Some(existing_progression) = self
            .reader
            .book_progression(book_id, user_id)
            .await
            .map_err(|_| DeviceProgressError::Persistence)?
        else {
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

    async fn koreader_epub_progress_value(&self, book_id: &str, locator: &Value) -> Option<String> {
        let href = locator.get("href").and_then(Value::as_str)?.trim();
        if href.is_empty() {
            return None;
        }

        let (_extension_class, blob) = self.reader.epub_extension_blob(book_id).await.ok()??;
        let extension = self.content.decode_epub_positions_extension(&blob).ok()?;
        let unique_hrefs = dedup_epub_hrefs(&extension.positions);

        unique_hrefs
            .iter()
            .position(|value| value == href)
            .map(|index| format!("/body/DocFragment[{}].0", index + 1))
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

fn parse_koreader_epub_resource_index(progress: &str) -> Option<usize> {
    let normalized = progress.trim().to_ascii_lowercase();

    if let Some(index) =
        parse_koreader_doc_fragment_index(normalized.as_str(), "docfragment[", ']', true)
    {
        return Some(index);
    }

    parse_koreader_doc_fragment_index(normalized.as_str(), "#_doc_fragment_", '_', false)
}

fn parse_koreader_doc_fragment_index(
    progress: &str,
    prefix: &str,
    suffix: char,
    one_based: bool,
) -> Option<usize> {
    let start = progress.find(prefix)? + prefix.len();
    let tail = &progress[start..];
    let end = tail.find(suffix)?;
    let index = tail[..end].parse::<usize>().ok()?;
    if one_based {
        index.checked_sub(1)
    } else {
        Some(index)
    }
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

fn normalized_href_base(href: &str) -> String {
    let base = href.split('#').next().unwrap_or(href).trim_end_matches('#');
    percent_decode(base).trim_start_matches('/').to_string()
}

fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
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

fn dedup_epub_hrefs(positions: &[Value]) -> Vec<String> {
    let mut unique_hrefs = Vec::<String>::new();
    for position in positions {
        let Some(position_href) = position.get("href").and_then(Value::as_str) else {
            continue;
        };
        let position_href = position_href.trim();
        if position_href.is_empty() || unique_hrefs.iter().any(|value| value == position_href) {
            continue;
        }
        unique_hrefs.push(position_href.to_string());
    }
    unique_hrefs
}

fn koreader_epub_locator(href: &str, matched_position: &Value) -> Value {
    let mut locator = json!({
        "href": href,
        "type": matched_position
            .get("type")
            .cloned()
            .unwrap_or_else(|| Value::String("application/xhtml+xml".to_string())),
        "locations": {
            "progression": 0.0,
            "totalProgression": matched_position
                .get("locations")
                .and_then(|value| value.get("totalProgression"))
                .cloned()
                .unwrap_or(Value::Null),
        },
    });

    if let Some(kobo_span) = matched_position.get("koboSpan").cloned()
        && !kobo_span.is_null()
    {
        locator
            .as_object_mut()
            .expect("koreader epub locator should be an object")
            .insert("koboSpan".to_string(), kobo_span);
    }

    locator
}
