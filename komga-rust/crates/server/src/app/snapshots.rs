use axum::http::{header, HeaderMap};
use serde_json::{json, Value};

use crate::app::CompatProfile;

pub(super) fn snapshot_json(path: &str, profile: CompatProfile) -> Value {
    let json = match path {
        "libraries-list-admin.json" => include_str!(
            "../../../../../komga/src/test/resources/compatibility-snapshots/rest/libraries-list-admin.json"
        ),
        "libraries-list-user.json" => include_str!(
            "../../../../../komga/src/test/resources/compatibility-snapshots/rest/libraries-list-user.json"
        ),
        "series-list.json" => {
            include_str!(
                "../../../../../komga/src/test/resources/compatibility-snapshots/rest/series-list.json"
            )
        }
        "books-list.json" => {
            include_str!(
                "../../../../../komga/src/test/resources/compatibility-snapshots/rest/books-list.json"
            )
        }
        "opds-v2-manifest.json" => include_str!(
            "../../../../../komga/src/test/resources/compatibility-snapshots/opds/opds-v2-manifest.json"
        ),
        other => panic!("unsupported snapshot: {other}"),
    };

    let mut value: Value = serde_json::from_str(json).expect("snapshot json should be valid");

    match (path, profile) {
        ("series-list.json", CompatProfile::JavaLiveLocaldb) => {
            if let Some(url) = value.pointer_mut("/content/0/url") {
                *url = Value::String(String::new());
            }
        }
        ("books-list.json", CompatProfile::JavaLiveLocaldb) => {
            if let Some(url) = value.pointer_mut("/content/0/url") {
                *url = Value::String("book.cbr".to_string());
            }
            if let Some(file_last_modified) = value.pointer_mut("/content/0/fileLastModified") {
                *file_last_modified = Value::String("2024-01-02T08:04:05Z".to_string());
            }
            if let Some(status) = value.pointer_mut("/content/0/media/status") {
                *status = Value::String("READY".to_string());
            }
            if let Some(media_type) = value.pointer_mut("/content/0/media/mediaType") {
                *media_type = Value::String("application/zip".to_string());
            }
            if let Some(pages_count) = value.pointer_mut("/content/0/media/pagesCount") {
                *pages_count = Value::Number(1.into());
            }
            if let Some(media_profile) = value.pointer_mut("/content/0/media/mediaProfile") {
                *media_profile = Value::String("DIVINA".to_string());
            }
        }
        _ => {}
    }

    value
}

pub(super) fn books_latest_json(profile: CompatProfile) -> Value {
    let mut value = snapshot_json("books-list.json", profile);

    if profile == CompatProfile::JavaLiveLocaldb {
        if let Some(sort_sorted) = value.pointer_mut("/sort/sorted") {
            *sort_sorted = Value::Bool(true);
        }
        if let Some(sort_unsorted) = value.pointer_mut("/sort/unsorted") {
            *sort_unsorted = Value::Bool(false);
        }
        if let Some(sort_empty) = value.pointer_mut("/sort/empty") {
            *sort_empty = Value::Bool(false);
        }
        if let Some(pageable_sort_sorted) = value.pointer_mut("/pageable/sort/sorted") {
            *pageable_sort_sorted = Value::Bool(true);
        }
        if let Some(pageable_sort_unsorted) = value.pointer_mut("/pageable/sort/unsorted") {
            *pageable_sort_unsorted = Value::Bool(false);
        }
        if let Some(pageable_sort_empty) = value.pointer_mut("/pageable/sort/empty") {
            *pageable_sort_empty = Value::Bool(false);
        }
    }

    value
}

pub(super) fn book_pages_json(_: CompatProfile) -> Value {
    json!([
        {
            "number": 1,
            "fileName": "komga.png",
            "mediaType": "image/png",
            "width": null,
            "height": null,
            "sizeBytes": 0,
            "size": "0 B",
        }
    ])
}

pub(super) fn opds_auth_json(headers: &HeaderMap) -> Value {
    let host = request_host(headers);

    json!({
        "authentication": [
            {
                "type": "http://opds-spec.org/auth/basic",
                "labels": {
                    "login": "Email",
                    "password": "Password"
                }
            }
        ],
        "title": "Komga",
        "id": absolute_url(&host, "/opds/v2/auth"),
        "description": "Enter your email and password to authenticate.",
        "links": [
            {
                "rel": "help",
                "href": "https://komga.org"
            },
            {
                "rel": "logo",
                "href": absolute_url(&host, "/android-chrome-512x512.png")
            }
        ]
    })
}

pub(super) fn request_host(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("localhost")
        .to_string()
}

fn absolute_url(host: &str, path: &str) -> String {
    format!("http://{host}{path}")
}
