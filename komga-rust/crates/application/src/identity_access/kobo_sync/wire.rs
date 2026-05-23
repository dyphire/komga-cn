use serde_json::{Value, json};

use super::super::device_records::KoboMetadataRecord;
use super::models::{KoboSyncBookSnapshot, KoboSyncReadListSnapshot, KoboSyncReadProgressSnapshot};
use super::sync_tokens::now_sync_marker;

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
    use serde_json::{Value, json};

    use super::*;
    use crate::identity_access::KoboMetadataRecord;

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
