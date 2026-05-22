use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use time::OffsetDateTime;

use super::device_records::KoboMetadataRecord;
use super::user_models::AuthUser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboStoreSyncMergeResult {
    pub events: Vec<Value>,
    pub raw_sync_token: Option<String>,
    pub should_continue: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboLibrarySyncRequest {
    pub user: AuthUser,
    pub current_api_key_id: Option<String>,
    pub sync_token_raw: Option<String>,
    pub store_sync_enabled: bool,
    pub forwarded_headers: Vec<(String, String)>,
    pub query: Option<String>,
    pub base_url: String,
    pub auth_token: String,
    pub limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboLibrarySyncResponse {
    pub events: Vec<Value>,
    pub sync_token_payload: String,
    pub should_continue: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboLibrarySyncPayload {
    pub events: Vec<Value>,
    pub encoded_sync_token: String,
    pub should_continue: bool,
}

pub fn is_kobo_store_sync_token_candidate(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.contains('.')
}

pub fn decode_or_passthrough_sync_token(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(normalized) = trimmed.strip_prefix("KOMGA.") {
        return STANDARD
            .decode(normalized)
            .ok()
            .or_else(|| STANDARD_NO_PAD.decode(normalized).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    if !trimmed.contains('.') {
        let decoded = STANDARD
            .decode(trimmed)
            .ok()
            .or_else(|| STANDARD_NO_PAD.decode(trimmed).ok())
            .and_then(|bytes| String::from_utf8(bytes).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        return decoded.and_then(|decoded| extract_calibre_web_raw_sync_token(&decoded));
    }

    Some(trimmed.to_string())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncBookSnapshot {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub release_date: Option<String>,
    pub language: String,
    pub file_size: u64,
    pub page_count: u64,
    pub created: String,
    pub last_modified: String,
    pub contributor_names: Vec<String>,
    pub isbn: Option<String>,
    pub publisher_name: Option<String>,
    pub cover_image_id: Option<String>,
    pub series_id: Option<String>,
    pub series_name: Option<String>,
    pub series_number: Option<String>,
    pub series_number_float: Option<f64>,
    pub oneshot: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncReadProgressSnapshot {
    pub page: i64,
    pub completed: bool,
    pub created: String,
    pub last_modified: String,
    pub locator: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncReadListSnapshot {
    pub id: String,
    pub name: String,
    pub created: String,
    pub last_modified: String,
    pub items: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct KoboSyncSnapshot {
    pub books: HashMap<String, KoboSyncBookSnapshot>,
    pub progress: HashMap<String, KoboSyncReadProgressSnapshot>,
    pub readlists: HashMap<String, KoboSyncReadListSnapshot>,
}

pub const KOBO_SYNC_ITEM_LIMIT: usize = 200;

fn kobo_description(summary: &str) -> Value {
    if summary.trim().is_empty() {
        Value::String(" ".to_string())
    } else {
        Value::String(summary.to_string())
    }
}

fn kobo_language(language: &str) -> String {
    let language = language.trim();
    if language.is_empty() {
        "en".to_string()
    } else {
        language
            .chars()
            .take(2)
            .collect::<String>()
            .to_ascii_lowercase()
    }
}

fn kobo_publication_date_value(value: &str) -> Option<Value> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else if value.len() == 10 && value.as_bytes().get(4) == Some(&b'-') {
        Some(Value::String(format!("{value}T00:00:00Z")))
    } else {
        Some(Value::String(value.to_string()))
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KomgaSyncTokenPayload {
    #[serde(default = "default_sync_token_version")]
    pub version: i32,
    #[serde(default, rename = "rawKoboSyncToken", alias = "raw_kobo_sync_token")]
    pub raw_kobo_sync_token: String,
    #[serde(
        default,
        rename = "ongoingSyncPointId",
        alias = "ongoing_sync_point_id"
    )]
    pub ongoing_sync_point_id: Option<String>,
    #[serde(
        default,
        rename = "lastSuccessfulSyncPointId",
        alias = "last_successful_sync_point_id"
    )]
    pub last_successful_sync_point_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboSyncPointBook {
    pub book_id: String,
    pub created: String,
    pub file_last_modified: String,
    pub file_size: u64,
    pub file_hash: String,
    pub metadata_last_modified: String,
    pub read_progress_last_modified: Option<String>,
    pub cover_image_id: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KoboSyncPage {
    pub to_sync_point_id: String,
    pub from_sync_point_id: Option<String>,
    pub books_added: Vec<KoboSyncPointBook>,
    pub books_changed: Vec<KoboSyncPointBook>,
    pub books_removed: Vec<KoboSyncPointBook>,
    pub books_read_progress_changed: Vec<KoboSyncPointBook>,
    pub readlists_added: Vec<KoboSyncReadListSnapshot>,
    pub readlists_changed: Vec<KoboSyncReadListSnapshot>,
    pub readlists_removed: Vec<KoboSyncReadListSnapshot>,
    pub should_continue: bool,
}

pub fn build_kobo_sync_events(
    from: Option<&KoboSyncSnapshot>,
    to: &KoboSyncSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Vec<Value> {
    let mut events = Vec::new();

    match from {
        None => {
            let mut books = to.books.values().collect::<Vec<_>>();
            books.sort_by(|a, b| a.id.cmp(&b.id));
            for book in books {
                events.push(kobo_new_entitlement_event(
                    book,
                    kobo_reading_state_from_snapshot(book, to.progress.get(&book.id)),
                    base_url,
                    auth_token,
                ));
            }

            let mut readlists = to.readlists.values().collect::<Vec<_>>();
            readlists.sort_by(|a, b| a.id.cmp(&b.id));
            for readlist in readlists {
                events.push(json!({
                    "NewTag": {
                        "Tag": kobo_tag_from_snapshot(readlist, true),
                    }
                }));
            }
        }
        Some(from) => {
            let mut to_book_ids = to.books.keys().cloned().collect::<Vec<_>>();
            to_book_ids.sort();
            for book_id in to_book_ids {
                let Some(to_book) = to.books.get(&book_id) else {
                    continue;
                };
                match from.books.get(&book_id) {
                    None => {
                        events.push(kobo_new_entitlement_event(
                            to_book,
                            kobo_reading_state_from_snapshot(to_book, to.progress.get(&book_id)),
                            base_url,
                            auth_token,
                        ));
                    }
                    Some(from_book) => {
                        if from_book.last_modified != to_book.last_modified {
                            events.push(kobo_new_entitlement_event(
                                to_book,
                                kobo_reading_state_from_snapshot(
                                    to_book,
                                    to.progress.get(&book_id),
                                ),
                                base_url,
                                auth_token,
                            ));
                            events.push(kobo_changed_product_metadata_event(
                                to_book, base_url, auth_token,
                            ));
                            if let Some(to_progress) = to.progress.get(&book_id) {
                                events.push(kobo_changed_reading_state_event(
                                    kobo_reading_state_from_snapshot(to_book, Some(to_progress)),
                                ));
                            }
                        }
                    }
                }
            }

            let mut removed_book_ids = from.books.keys().cloned().collect::<Vec<_>>();
            removed_book_ids.sort();
            for book_id in removed_book_ids {
                if to.books.contains_key(&book_id) {
                    continue;
                }
                if let Some(from_book) = from.books.get(&book_id) {
                    events.push(kobo_changed_entitlement_removed_event(
                        from_book, base_url, auth_token,
                    ));
                }
            }

            let mut progress_book_ids = to
                .progress
                .keys()
                .chain(from.progress.keys())
                .cloned()
                .collect::<Vec<_>>();
            progress_book_ids.sort();
            progress_book_ids.dedup();
            for book_id in progress_book_ids {
                let from_progress = from.progress.get(&book_id);
                let to_progress = to.progress.get(&book_id);
                if from_progress.map(|value| {
                    (
                        &value.last_modified,
                        value.page,
                        value.completed,
                        value.locator.as_ref(),
                    )
                }) == to_progress.map(|value| {
                    (
                        &value.last_modified,
                        value.page,
                        value.completed,
                        value.locator.as_ref(),
                    )
                }) {
                    continue;
                }
                if let Some(book) = to.books.get(&book_id)
                    && let Some(progress) = to_progress
                {
                    events.push(kobo_changed_reading_state_event(
                        kobo_reading_state_from_snapshot(book, Some(progress)),
                    ));
                }
            }

            let mut to_readlist_ids = to.readlists.keys().cloned().collect::<Vec<_>>();
            to_readlist_ids.sort();
            for readlist_id in to_readlist_ids {
                let Some(to_readlist) = to.readlists.get(&readlist_id) else {
                    continue;
                };
                match from.readlists.get(&readlist_id) {
                    None => events.push(json!({
                        "NewTag": {
                            "Tag": kobo_tag_from_snapshot(to_readlist, true),
                        }
                    })),
                    Some(from_readlist)
                        if from_readlist.last_modified != to_readlist.last_modified
                            || from_readlist.name != to_readlist.name
                            || from_readlist.items != to_readlist.items =>
                    {
                        events.push(json!({
                            "ChangedTag": {
                                "Tag": kobo_tag_from_snapshot(to_readlist, true),
                            }
                        }));
                    }
                    Some(_) => {}
                }
            }

            let mut removed_readlists = from.readlists.keys().cloned().collect::<Vec<_>>();
            removed_readlists.sort();
            for readlist_id in removed_readlists {
                if to.readlists.contains_key(&readlist_id) {
                    continue;
                }
                let Some(previous) = from.readlists.get(&readlist_id) else {
                    continue;
                };
                events.push(json!({
                    "DeletedTag": {
                        "Tag": kobo_tag_from_snapshot(previous, false),
                    }
                }));
            }
        }
    }

    events
}

pub fn build_kobo_library_sync_payload(
    response: KoboLibrarySyncResponse,
) -> KoboLibrarySyncPayload {
    KoboLibrarySyncPayload {
        events: response.events,
        encoded_sync_token: format!(
            "KOMGA.{}",
            STANDARD_NO_PAD.encode(response.sync_token_payload)
        ),
        should_continue: response.should_continue,
    }
}

pub fn build_kobo_book_metadata_payload(
    book_id: &str,
    metadata: &KoboMetadataRecord,
    base_url: &str,
    auth_token: &str,
) -> Value {
    let (format, convert_kepub) = if metadata.is_pre_paginated {
        ("EPUB3FL", false)
    } else {
        ("KEPUB", !metadata.is_kepub)
    };

    Value::Array(vec![kobo_book_metadata_wire(KoboBookMetadataWireInput {
        id: book_id,
        title: &metadata.title,
        summary: &metadata.summary,
        publication_date: metadata.release_date.as_deref(),
        publication_fallback_date: metadata.created_date.as_deref(),
        language: &metadata.language,
        file_size: metadata.file_size,
        contributor_names: &metadata.contributor_names,
        isbn: metadata.isbn.as_deref(),
        publisher_name: metadata.publisher_name.as_deref(),
        cover_image_id: metadata.cover_image_id.as_deref(),
        series_id: metadata.series_id.as_deref(),
        series_name: metadata.series_name.as_deref(),
        series_number: metadata.series_number.as_deref(),
        series_number_float: metadata.series_number_float,
        oneshot: metadata.oneshot,
        download_format: format,
        download_url: format!(
            "{base_url}/kobo/{auth_token}/v1/books/{book_id}/file/epub?convert_kepub={convert_kepub}"
        ),
        include_standalone_fields: true,
    })])
}

pub fn build_kobo_new_entitlement(
    book: &KoboSyncBookSnapshot,
    progress: Option<&KoboSyncReadProgressSnapshot>,
    base_url: &str,
    auth_token: &str,
) -> Value {
    kobo_new_entitlement_event(
        book,
        kobo_reading_state_from_snapshot(book, progress),
        base_url,
        auth_token,
    )
}

pub fn build_kobo_changed_product_metadata(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    kobo_changed_product_metadata_event(book, base_url, auth_token)
}

pub fn build_kobo_changed_entitlement_removed(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    kobo_changed_entitlement_removed_event(book, base_url, auth_token)
}

pub fn build_kobo_changed_reading_state(
    book: &KoboSyncBookSnapshot,
    progress: &KoboSyncReadProgressSnapshot,
) -> Value {
    kobo_changed_reading_state_event(kobo_reading_state_from_snapshot(book, Some(progress)))
}

pub fn build_kobo_new_tag(readlist: &KoboSyncReadListSnapshot) -> Value {
    json!({
        "NewTag": {
            "Tag": kobo_tag_from_snapshot(readlist, true),
        }
    })
}

pub fn build_kobo_changed_tag(readlist: &KoboSyncReadListSnapshot) -> Value {
    json!({
        "ChangedTag": {
            "Tag": kobo_tag_from_snapshot(readlist, true),
        }
    })
}

pub fn build_kobo_deleted_tag(readlist: &KoboSyncReadListSnapshot) -> Value {
    json!({
        "DeletedTag": {
            "Tag": kobo_tag_from_snapshot(readlist, false),
        }
    })
}

pub fn now_sync_marker() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "2000-01-01T00:00:00Z".to_string())
}

pub fn parse_komga_sync_token_payload(value: &str) -> Option<KomgaSyncTokenPayload> {
    serde_json::from_str::<KomgaSyncTokenPayload>(value).ok()
}

pub fn build_komga_sync_token_payload(
    previous: Option<KomgaSyncTokenPayload>,
    incoming_raw_sync_token: Option<String>,
    sync_point_id: &str,
    should_continue: bool,
) -> String {
    let mut payload = previous.unwrap_or_default();
    if payload.version <= 0 {
        payload.version = default_sync_token_version();
    }
    if payload.raw_kobo_sync_token.is_empty()
        && let Some(raw) = incoming_raw_sync_token
    {
        payload.raw_kobo_sync_token = raw;
    }
    if should_continue {
        payload.ongoing_sync_point_id = Some(sync_point_id.to_string());
    } else {
        let finalized_sync_point = payload
            .ongoing_sync_point_id
            .clone()
            .unwrap_or_else(|| sync_point_id.to_string());
        payload.ongoing_sync_point_id = None;
        payload.last_successful_sync_point_id = Some(finalized_sync_point);
    }
    serde_json::to_string(&payload).unwrap_or_else(|_| {
        json!({
            "version": default_sync_token_version(),
            "rawKoboSyncToken": "",
            "ongoingSyncPointId": if should_continue { Value::String(sync_point_id.to_string()) } else { Value::Null },
            "lastSuccessfulSyncPointId": if should_continue { Value::Null } else { Value::String(sync_point_id.to_string()) },
        })
        .to_string()
    })
}

fn default_sync_token_version() -> i32 {
    1
}

fn extract_calibre_web_raw_sync_token(decoded_token: &str) -> Option<String> {
    serde_json::from_str::<Value>(decoded_token)
        .ok()
        .and_then(|value| value.get("data").cloned())
        .and_then(|value| {
            value
                .get("raw_kobo_store_token")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

fn kobo_reading_state_from_snapshot(
    book: &KoboSyncBookSnapshot,
    progress: Option<&KoboSyncReadProgressSnapshot>,
) -> Value {
    if let Some(progress) = progress {
        let locator = parse_locator_payload(progress.locator.as_deref());
        let source_progress = locator
            .get("locations")
            .and_then(|value| value.get("progression"))
            .and_then(Value::as_f64);
        let total_progress = locator
            .get("locations")
            .and_then(|value| value.get("totalProgression"))
            .and_then(Value::as_f64);
        let source = locator
            .get("href")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let value = locator
            .get("koboSpan")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let mut bookmark = serde_json::Map::new();
        bookmark.insert(
            "LastModified".to_string(),
            Value::String(progress.last_modified.clone()),
        );
        if let Some(total_progress) = total_progress {
            bookmark.insert("ProgressPercent".to_string(), json!(total_progress * 100.0));
        }
        if let Some(source_progress) = source_progress {
            bookmark.insert(
                "ContentSourceProgressPercent".to_string(),
                json!(source_progress * 100.0),
            );
        }
        if let Some(source) = source {
            bookmark.insert(
                "Location".to_string(),
                json!({
                    "Source": source,
                    "Type": "KoboSpan",
                    "Value": value,
                }),
            );
        }
        let status = if progress.completed {
            "Finished"
        } else {
            "Reading"
        };
        json!({
            "Created": progress.created,
            "CurrentBookmark": Value::Object(bookmark),
            "EntitlementId": book.id,
            "LastModified": progress.last_modified,
            "PriorityTimestamp": progress.last_modified,
            "Statistics": {
                "LastModified": progress.last_modified,
            },
            "StatusInfo": {
                "LastModified": progress.last_modified,
                "Status": status,
                "TimesStartedReading": 1,
            },
        })
    } else {
        json!({
            "Created": book.created,
            "CurrentBookmark": {
                "LastModified": book.created,
            },
            "EntitlementId": book.id,
            "LastModified": book.created,
            "PriorityTimestamp": book.created,
            "Statistics": {
                "LastModified": book.created,
            },
            "StatusInfo": {
                "LastModified": book.created,
                "Status": "ReadyToRead",
                "TimesStartedReading": 0,
            },
        })
    }
}

struct KoboBookMetadataWireInput<'a> {
    id: &'a str,
    title: &'a str,
    summary: &'a str,
    publication_date: Option<&'a str>,
    publication_fallback_date: Option<&'a str>,
    language: &'a str,
    file_size: u64,
    contributor_names: &'a [String],
    isbn: Option<&'a str>,
    publisher_name: Option<&'a str>,
    cover_image_id: Option<&'a str>,
    series_id: Option<&'a str>,
    series_name: Option<&'a str>,
    series_number: Option<&'a str>,
    series_number_float: Option<f64>,
    oneshot: bool,
    download_format: &'a str,
    download_url: String,
    include_standalone_fields: bool,
}

fn kobo_book_metadata_wire(input: KoboBookMetadataWireInput<'_>) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert(
        "Categories".to_string(),
        Value::Array(vec![Value::String(
            "00000000-0000-0000-0000-000000000001".to_string(),
        )]),
    );
    metadata.insert(
        "ContributorRoles".to_string(),
        Value::Array(
            input
                .contributor_names
                .iter()
                .map(|name| json!({ "Name": name }))
                .collect(),
        ),
    );
    metadata.insert(
        "Contributors".to_string(),
        Value::Array(
            input
                .contributor_names
                .iter()
                .map(|name| Value::String(name.clone()))
                .collect(),
        ),
    );
    metadata.insert(
        "CoverImageId".to_string(),
        input
            .cover_image_id
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "CrossRevisionId".to_string(),
        Value::String(input.id.to_string()),
    );
    metadata.insert(
        "CurrentDisplayPrice".to_string(),
        json!({"CurrencyCode": "USD", "TotalAmount": 0}),
    );
    metadata.insert(
        "CurrentLoveDisplayPrice".to_string(),
        json!({"CurrencyCode": "USD", "TotalAmount": 0}),
    );
    metadata.insert("Description".to_string(), kobo_description(input.summary));
    metadata.insert(
        "DownloadUrls".to_string(),
        json!([
            {
                "DrmType": "None",
                "Format": input.download_format,
                "Platform": "Generic",
                "Size": input.file_size,
                "Url": input.download_url,
            }
        ]),
    );
    metadata.insert(
        "EntitlementId".to_string(),
        Value::String(input.id.to_string()),
    );
    metadata.insert("ExternalIds".to_string(), Value::Array(vec![]));
    metadata.insert(
        "Genre".to_string(),
        Value::String("00000000-0000-0000-0000-000000000001".to_string()),
    );
    metadata.insert("IsEligibleForKoboLove".to_string(), Value::Bool(false));
    metadata.insert("IsInternetArchive".to_string(), Value::Bool(false));
    metadata.insert("IsPreOrder".to_string(), Value::Bool(false));
    metadata.insert("IsSocialEnabled".to_string(), Value::Bool(true));
    metadata.insert(
        "ISBN".to_string(),
        input
            .isbn
            .map(|value| Value::String(value.to_string()))
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "Language".to_string(),
        Value::String(kobo_language(input.language)),
    );
    metadata.insert(
        "PhoneticPronunciations".to_string(),
        Value::Object(serde_json::Map::new()),
    );
    metadata.insert(
        "PublicationDate".to_string(),
        input
            .publication_date
            .or(input.publication_fallback_date)
            .and_then(kobo_publication_date_value)
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "Publisher".to_string(),
        input
            .publisher_name
            .map(|name| json!({ "Imprint": "", "Name": name }))
            .unwrap_or(Value::Null),
    );
    metadata.insert(
        "RevisionId".to_string(),
        Value::String(input.id.to_string()),
    );
    metadata.insert(
        "Series".to_string(),
        if input.oneshot {
            Value::Null
        } else if let (
            Some(series_id),
            Some(series_name),
            Some(series_number),
            Some(series_number_float),
        ) = (
            input.series_id,
            input.series_name,
            input.series_number,
            input.series_number_float,
        ) {
            json!({
                "Id": series_id,
                "Name": series_name,
                "Number": series_number,
                "NumberFloat": series_number_float,
            })
        } else {
            Value::Null
        },
    );
    if input.include_standalone_fields {
        metadata.insert("Slug".to_string(), Value::Null);
        metadata.insert("SubTitle".to_string(), Value::Null);
    }
    metadata.insert("Title".to_string(), Value::String(input.title.to_string()));
    metadata.insert("WorkId".to_string(), Value::String(input.id.to_string()));
    Value::Object(metadata)
}

fn kobo_book_metadata_from_snapshot(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    kobo_book_metadata_wire(KoboBookMetadataWireInput {
        id: &book.id,
        title: &book.title,
        summary: &book.summary,
        publication_date: book.release_date.as_deref(),
        publication_fallback_date: Some(book.created.as_str()),
        language: &book.language,
        file_size: book.file_size,
        contributor_names: &book.contributor_names,
        isbn: book.isbn.as_deref(),
        publisher_name: book.publisher_name.as_deref(),
        cover_image_id: book.cover_image_id.as_deref(),
        series_id: book.series_id.as_deref(),
        series_name: book.series_name.as_deref(),
        series_number: book.series_number.as_deref(),
        series_number_float: book.series_number_float,
        oneshot: book.oneshot,
        download_format: "EPUB",
        download_url: format!(
            "{base_url}/kobo/{auth_token}/v1/books/{}/file/epub",
            book.id
        ),
        include_standalone_fields: false,
    })
}

fn kobo_entitlement_from_snapshot(book: &KoboSyncBookSnapshot, is_removed: bool) -> Value {
    json!({
        "Accessibility": "Full",
        "ActivePeriod": {
            "From": now_sync_marker(),
        },
        "Created": book.created,
        "CrossRevisionId": book.id,
        "Id": book.id,
        "IsHiddenFromArchive": false,
        "IsLocked": false,
        "IsRemoved": is_removed,
        "LastModified": book.last_modified,
        "OriginCategory": "Imported",
        "RevisionId": book.id,
        "Status": "Active",
    })
}

fn kobo_tag_from_snapshot(readlist: &KoboSyncReadListSnapshot, include_items: bool) -> Value {
    let mut tag = serde_json::Map::new();
    tag.insert("Id".to_string(), Value::String(readlist.id.clone()));
    tag.insert(
        "Created".to_string(),
        Value::String(readlist.created.clone()),
    );
    tag.insert(
        "LastModified".to_string(),
        Value::String(readlist.last_modified.clone()),
    );
    tag.insert("Name".to_string(), Value::String(readlist.name.clone()));
    tag.insert("Type".to_string(), Value::String("UserTag".to_string()));
    if include_items {
        let items = readlist
            .items
            .iter()
            .map(|book_id| {
                json!({
                    "RevisionId": book_id,
                    "Type": "ProductRevisionTagItem",
                })
            })
            .collect::<Vec<_>>();
        tag.insert("Items".to_string(), Value::Array(items));
    }
    Value::Object(tag)
}

fn kobo_new_entitlement_event(
    book: &KoboSyncBookSnapshot,
    reading_state: Value,
    base_url: &str,
    auth_token: &str,
) -> Value {
    json!({
        "NewEntitlement": {
            "BookEntitlement": kobo_entitlement_from_snapshot(book, false),
            "BookMetadata": kobo_book_metadata_from_snapshot(book, base_url, auth_token),
            "ReadingState": reading_state,
        }
    })
}

fn kobo_changed_entitlement_removed_event(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    json!({
        "ChangedEntitlement": {
            "BookEntitlement": kobo_entitlement_from_snapshot(book, true),
            "BookMetadata": kobo_book_metadata_from_snapshot(book, base_url, auth_token),
        }
    })
}

fn kobo_changed_product_metadata_event(
    book: &KoboSyncBookSnapshot,
    base_url: &str,
    auth_token: &str,
) -> Value {
    json!({
        "ChangedProductMetadata": kobo_book_metadata_from_snapshot(book, base_url, auth_token),
    })
}

fn kobo_changed_reading_state_event(reading_state: Value) -> Value {
    json!({
        "ChangedReadingState": {
            "ReadingState": reading_state,
        }
    })
}

fn parse_locator_payload(locator: Option<&[u8]>) -> Value {
    locator
        .and_then(|blob| serde_json::from_slice::<Value>(blob).ok())
        .unwrap_or_else(|| json!({}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_access::KoboMetadataRecord;

    #[test]
    fn kobo_protocol_payload_encodes_library_sync_token() {
        let payload = build_kobo_library_sync_payload(KoboLibrarySyncResponse {
            events: vec![json!({ "NewTag": {} })],
            sync_token_payload: r#"{"version":1,"rawKoboSyncToken":""}"#.to_string(),
            should_continue: true,
        });

        assert_eq!(
            payload.encoded_sync_token,
            format!(
                "KOMGA.{}",
                STANDARD_NO_PAD.encode(r#"{"version":1,"rawKoboSyncToken":""}"#)
            )
        );
        assert_eq!(payload.events, vec![json!({ "NewTag": {} })]);
        assert!(payload.should_continue);
    }

    #[test]
    fn kobo_protocol_payload_builds_standalone_book_metadata() {
        let metadata = KoboMetadataRecord {
            title: "Book One".to_string(),
            summary: String::new(),
            release_date: Some("2026-02-03".to_string()),
            created_date: Some("2026-01-01T00:00:00Z".to_string()),
            language: "FR-ca".to_string(),
            file_size: 1234,
            file_name: "book.epub".to_string(),
            media_type: "application/epub+zip".to_string(),
            contributor_names: vec!["Jane Writer".to_string()],
            isbn: Some("9781234567890".to_string()),
            publisher_name: Some("PubHouse".to_string()),
            cover_image_id: Some("cover-1".to_string()),
            series_id: Some("series-1".to_string()),
            series_name: Some("Series One".to_string()),
            series_number: Some("1".to_string()),
            series_number_float: Some(1.0),
            oneshot: false,
            is_kepub: false,
            is_pre_paginated: false,
        };

        let payload = build_kobo_book_metadata_payload(
            "book-1",
            &metadata,
            "http://localhost:8080",
            "token-1",
        );
        let book = payload
            .as_array()
            .and_then(|items| items.first())
            .expect("metadata item expected");

        assert_eq!(
            book.get("Description"),
            Some(&Value::String(" ".to_string()))
        );
        assert_eq!(book.get("Language"), Some(&Value::String("fr".to_string())));
        assert_eq!(
            book.get("PublicationDate"),
            Some(&Value::String("2026-02-03T00:00:00Z".to_string()))
        );
        assert_eq!(
            book.get("DownloadUrls")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|download| download.get("Format")),
            Some(&Value::String("KEPUB".to_string()))
        );
        assert_eq!(
            book.get("DownloadUrls")
                .and_then(Value::as_array)
                .and_then(|items| items.first())
                .and_then(|download| download.get("Url")),
            Some(&Value::String(
                "http://localhost:8080/kobo/token-1/v1/books/book-1/file/epub?convert_kepub=true"
                    .to_string()
            ))
        );
        assert_eq!(
            book.get("Series")
                .and_then(|series| series.get("NumberFloat")),
            Some(&json!(1.0))
        );
        assert_eq!(book.get("Slug"), Some(&Value::Null));
        assert_eq!(book.get("SubTitle"), Some(&Value::Null));
    }
}
