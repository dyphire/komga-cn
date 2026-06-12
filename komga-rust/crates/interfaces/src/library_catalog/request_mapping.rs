#![allow(clippy::result_large_err)]

use crate::helpers::validation_error_response;
use axum::response::Response;
use komga_application::library_catalog::{
    LibraryChangeSet, LibraryScanInterval, LibrarySeriesCover,
};
use serde_json::{Value, json};

use super::handlers::bad_request_response;

pub(super) fn parse_create_library_change_set(body: &Value) -> Result<LibraryChangeSet, Response> {
    let changes = parse_library_change_set(body, "library create payload must be a JSON object")?;
    if changes.name.is_none() || changes.root.is_none() {
        return Err(bad_request_response(
            "library create payload must include name and root",
        ));
    }
    if changes
        .name
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
        || changes
            .root
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
    {
        return Err(bad_request_response(
            "library create payload must provide non-empty name and root",
        ));
    }
    Ok(changes)
}

pub(super) fn parse_update_library_change_set(body: &Value) -> Result<LibraryChangeSet, Response> {
    let mut normalized = match body.as_object() {
        Some(body) => body.clone(),
        None => {
            return Err(bad_request_response(
                "library update payload must be a JSON object",
            ));
        }
    };

    let mut violations = Vec::new();
    normalize_nullable_patch_string_field(&mut normalized, "root", &mut violations)?;
    normalize_nullable_patch_string_field(&mut normalized, "name", &mut violations)?;
    if !violations.is_empty() {
        return Err(validation_error_response(violations));
    }
    normalize_nullable_patch_string_array_field(&mut normalized, "scanDirectoryExclusions")?;

    parse_library_change_set(
        &Value::Object(normalized),
        "library update payload must be a JSON object",
    )
}

pub(super) fn is_deep_scan_query(query: &str) -> bool {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .any(|(key, value)| {
            key.eq_ignore_ascii_case("deep")
                && matches!(value.to_ascii_lowercase().as_str(), "true" | "1")
        })
}

fn parse_library_change_set(
    body: &Value,
    invalid_message: &str,
) -> Result<LibraryChangeSet, Response> {
    let body = match body.as_object() {
        Some(body) => body,
        None => return Err(bad_request_response(invalid_message)),
    };

    let mut changes = LibraryChangeSet::default();
    apply_string_field(body, "name", &mut changes.name)?;
    apply_string_field(body, "root", &mut changes.root)?;
    apply_bool_field(
        body,
        "importComicInfoBook",
        &mut changes.import_comicinfo_book,
    )?;
    apply_bool_field(
        body,
        "importComicInfoSeries",
        &mut changes.import_comicinfo_series,
    )?;
    apply_bool_field(
        body,
        "importComicInfoCollection",
        &mut changes.import_comicinfo_collection,
    )?;
    apply_bool_field(
        body,
        "importComicInfoReadList",
        &mut changes.import_comicinfo_readlist,
    )?;
    apply_bool_field(
        body,
        "importComicInfoSeriesAppendVolume",
        &mut changes.import_comicinfo_series_append_volume,
    )?;
    apply_bool_field(body, "importEpubBook", &mut changes.import_epub_book)?;
    apply_bool_field(body, "importEpubSeries", &mut changes.import_epub_series)?;
    apply_bool_field(body, "importMylarSeries", &mut changes.import_mylar_series)?;
    apply_bool_field(
        body,
        "importLocalArtwork",
        &mut changes.import_local_artwork,
    )?;
    apply_bool_field(body, "importBarcodeIsbn", &mut changes.import_barcode_isbn)?;
    apply_bool_field(
        body,
        "scanForceModifiedTime",
        &mut changes.scan_force_modified_time,
    )?;
    apply_scan_interval_field(body, "scanInterval", &mut changes.scan_interval)?;
    apply_bool_field(body, "scanOnStartup", &mut changes.scan_on_startup)?;
    apply_bool_field(body, "scanCbx", &mut changes.scan_cbx)?;
    apply_bool_field(body, "scanPdf", &mut changes.scan_pdf)?;
    apply_bool_field(body, "scanEpub", &mut changes.scan_epub)?;
    apply_string_array_field(
        body,
        "scanDirectoryExclusions",
        &mut changes.scan_directory_exclusions,
    )?;
    apply_bool_field(body, "repairExtensions", &mut changes.repair_extensions)?;
    apply_bool_field(body, "convertToCbz", &mut changes.convert_to_cbz)?;
    apply_bool_field(
        body,
        "emptyTrashAfterScan",
        &mut changes.empty_trash_after_scan,
    )?;
    apply_series_cover_field(body, "seriesCover", &mut changes.series_cover)?;
    apply_bool_field(body, "hashFiles", &mut changes.hash_files)?;
    apply_bool_field(body, "hashPages", &mut changes.hash_pages)?;
    apply_bool_field(body, "hashKoreader", &mut changes.hash_koreader)?;
    apply_bool_field(body, "analyzeDimensions", &mut changes.analyze_dimensions)?;
    apply_optional_string_field(body, "oneshotsDirectory", &mut changes.oneshots_directory)?;
    Ok(changes)
}

fn apply_string_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<String>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!("{key} must be a string")));
    };
    *field = Some(value.to_string());
    Ok(())
}

fn apply_scan_interval_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<LibraryScanInterval>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!("{key} must be a string")));
    };
    let Some(scan_interval) = LibraryScanInterval::from_persisted_name(value) else {
        return Err(bad_request_response(&format!("{key} has an invalid value")));
    };
    *field = Some(scan_interval);
    Ok(())
}

fn apply_series_cover_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<LibrarySeriesCover>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!("{key} must be a string")));
    };
    let Some(series_cover) = LibrarySeriesCover::from_persisted_name(value) else {
        return Err(bad_request_response(&format!("{key} has an invalid value")));
    };
    *field = Some(series_cover);
    Ok(())
}

fn apply_bool_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<bool>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(value) = value.as_bool() else {
        return Err(bad_request_response(&format!("{key} must be a boolean")));
    };
    *field = Some(value);
    Ok(())
}

fn apply_optional_string_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<Option<String>>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    if value.is_null() {
        *field = Some(None);
        return Ok(());
    }
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!(
            "{key} must be a string or null"
        )));
    };
    *field = Some(if value.chars().all(|ch| ch.is_whitespace()) {
        None
    } else {
        Some(value.to_string())
    });
    Ok(())
}

fn apply_string_array_field(
    body: &serde_json::Map<String, Value>,
    key: &str,
    field: &mut Option<Vec<String>>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    let Some(values) = value.as_array() else {
        return Err(bad_request_response(&format!(
            "{key} must be an array of strings"
        )));
    };
    let mut next = Vec::with_capacity(values.len());
    for entry in values {
        let Some(entry) = entry.as_str() else {
            return Err(bad_request_response(&format!(
                "{key} must be an array of strings"
            )));
        };
        next.push(entry.to_string());
    }
    *field = Some(next);
    Ok(())
}

fn normalize_nullable_patch_string_field(
    body: &mut serde_json::Map<String, Value>,
    key: &str,
    violations: &mut Vec<Value>,
) -> Result<(), Response> {
    let Some(value) = body.get(key) else {
        return Ok(());
    };
    if value.is_null() {
        body.remove(key);
        return Ok(());
    }
    let Some(value) = value.as_str() else {
        return Err(bad_request_response(&format!("{key} must be a string")));
    };
    if value.trim().is_empty() {
        violations.push(json!({
            "fieldName": key,
            "message": "must not be blank",
        }));
    }
    Ok(())
}

fn normalize_nullable_patch_string_array_field(
    body: &mut serde_json::Map<String, Value>,
    key: &str,
) -> Result<(), Response> {
    let Some(value) = body.get_mut(key) else {
        return Ok(());
    };
    if value.is_null() {
        *value = Value::Array(vec![]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axum::http::StatusCode;
    use komga_application::library_catalog::{LibraryScanInterval, LibrarySeriesCover};
    use serde_json::json;

    use super::{parse_create_library_change_set, parse_update_library_change_set};

    #[test]
    fn create_library_accepts_known_enum_values() {
        let payload = json!({
            "name": "Library",
            "root": "/books",
            "scanInterval": "EVERY_12H",
            "seriesCover": "FIRST_UNREAD_OR_LAST"
        });

        let changes = parse_create_library_change_set(&payload)
            .expect("known enum values should be accepted");

        assert_eq!(changes.scan_interval, Some(LibraryScanInterval::Every12h));
        assert_eq!(
            changes.series_cover,
            Some(LibrarySeriesCover::FirstUnreadOrLast)
        );
    }

    #[test]
    fn create_library_rejects_unknown_scan_interval() {
        let payload = json!({
            "name": "Library",
            "root": "/books",
            "scanInterval": "SOMEDAY"
        });

        let response = parse_create_library_change_set(&payload)
            .expect_err("unknown scanInterval should be rejected");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn create_library_rejects_unknown_series_cover() {
        let payload = json!({
            "name": "Library",
            "root": "/books",
            "seriesCover": "MIDDLE"
        });

        let response = parse_create_library_change_set(&payload)
            .expect_err("unknown seriesCover should be rejected");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn create_library_rejects_blank_name_or_root() {
        let payload = json!({
            "name": "   ",
            "root": "/books"
        });

        let response = parse_create_library_change_set(&payload)
            .expect_err("blank name should be rejected before persistence");

        assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    }

    #[test]
    fn update_library_accepts_null_scan_directory_exclusions_as_clear() {
        let payload = json!({
            "scanDirectoryExclusions": null
        });

        let changes = parse_update_library_change_set(&payload)
            .expect("null scanDirectoryExclusions should clear exclusions on PATCH");

        assert_eq!(changes.scan_directory_exclusions, Some(Vec::new()));
    }

    #[test]
    fn update_library_rejects_blank_name_or_root() {
        let payload = json!({
            "name": "   "
        });

        let response = parse_update_library_change_set(&payload)
            .expect_err("blank PATCH name should be rejected before persistence");

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn update_library_accepts_null_name_or_root_as_noop() {
        let payload = json!({
            "name": null,
            "root": null
        });

        let changes = parse_update_library_change_set(&payload)
            .expect("null PATCH name/root should be treated like omitted fields");

        assert_eq!(changes.name, None);
        assert_eq!(changes.root, None);
    }
}
