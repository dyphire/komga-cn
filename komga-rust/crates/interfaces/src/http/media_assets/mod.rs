use std::collections::BTreeSet;
use std::path::{Path as FsPath, PathBuf};

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Extension, Path, Query};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use image::ImageFormat;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::http::cache::{
    asset_etag, asset_not_modified_response, asset_ok_response, file_last_modified_header_value,
    if_modified_since_matches, if_none_match_matches,
};
use crate::http::discovery::{resolve_book_id_for_persisted, resolve_series_id_for_persisted};
use crate::http::discovery_auth::principal_from_user_payload;
use crate::http::identity_access::auth::{
    AuthUser, require_admin, require_auth, require_file_download, resolved_auth_user,
    resolved_token, user_has_role, user_id, user_is_admin, user_payload_json,
    user_shared_all_libraries, user_shared_library_ids,
};
use crate::http::request_urls::app_absolute_url;
use crate::http::state::AuthDatabaseState;
use crate::http::state::RuntimeProfile;
use crate::media_assets_runtime_access::*;
use komga_application::task_processing::TaskQueueRecord;

use super::super::{OperationalState, ReadProgressState};
use super::helpers::{
    invalid_progression_payload, invalid_read_progress_payload, mark_runtime_owned,
    method_not_allowed_json_response, set_read_progress,
};

#[path = "access_control.rs"]
mod access_control;
#[path = "archive_payload.rs"]
mod archive_payload;
#[path = "epub_positions.rs"]
mod epub_positions;
#[path = "files.rs"]
mod files;
#[path = "handlers.rs"]
mod handlers;
#[path = "http_helpers.rs"]
mod http_helpers;
#[path = "import.rs"]
mod import;
#[path = "import_internals.rs"]
mod import_internals;
#[path = "manifest_persistence.rs"]
mod manifest_persistence;
#[path = "manifests.rs"]
mod manifests;
#[path = "media_helpers.rs"]
mod media_helpers;
#[path = "operations.rs"]
mod operations;
#[path = "pages.rs"]
mod pages;
#[path = "read_progress.rs"]
mod read_progress;
#[path = "thumbnails.rs"]
mod thumbnails;
#[path = "types.rs"]
mod types;

pub use handlers::*;

use access_control::*;
use archive_payload::*;
use epub_positions::*;
use http_helpers::*;
use import_internals::*;
use manifest_persistence::*;
use media_helpers::*;
use types::*;

fn process_task_side_effects(
    state: &super::super::OperationalState,
    task_records: Vec<TaskQueueRecord>,
) -> Result<(), String> {
    (state.enqueue_task_records)(task_records, true)
}

fn enqueue_task_records(
    state: &super::super::OperationalState,
    task_records: Vec<TaskQueueRecord>,
) -> Response {
    if let Err(error) = process_task_side_effects(state, task_records) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": error })),
        )
            .into_response();
    }

    let mut response = StatusCode::ACCEPTED.into_response();
    mark_runtime_owned(&mut response);
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::cache::format_http_date;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use lopdf::{Document as PdfDocument, Object, Stream, dictionary};
    use std::fs;
    use std::io::Write;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_path(prefix: &str) -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_millis();
        std::env::temp_dir().join(format!("{prefix}-{millis}"))
    }

    fn assert_f64_close(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    fn write_single_page_pdf(path: &std::path::Path) {
        let mut document = PdfDocument::with_version("1.5");
        let pages_id = document.new_object_id();
        let page_id = document.new_object_id();
        let content_id = document.add_object(Stream::new(dictionary! {}, Vec::new()));
        let resources_id = document.add_object(dictionary! {});

        document.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
                "Contents" => content_id,
                "Resources" => resources_id,
            }),
        );
        document.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );

        let catalog_id = document.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        document.trailer.set("Root", catalog_id);
        document.compress();
        document
            .save(path)
            .expect("single-page pdf should be saved");
    }

    #[test]
    fn page_api_support_depends_on_image_or_known_page_count() {
        let image_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: PathBuf::from("/tmp/cover.jpg"),
            media_type: "image/jpeg".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&image_media));

        let paged_archive = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/tmp/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 25,
        };
        assert!(book_media_supports_page_api(&paged_archive));

        let unknown_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: PathBuf::from("/tmp/book.cbz"),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&unknown_media));

        let rar_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbr".to_string(),
            file_path: PathBuf::from("/tmp/book.cbr"),
            media_type: "application/vnd.comicbook-rar".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&rar_media));

        let pdf_media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: PathBuf::from("/tmp/book.pdf"),
            media_type: "application/pdf".to_string(),
            page_count: 0,
        };
        assert!(book_media_supports_page_api(&pdf_media));
    }

    #[test]
    fn if_modified_since_uses_http_date_ordering() {
        let resource_time = UNIX_EPOCH + std::time::Duration::from_secs(10);
        let expected_last_modified =
            format_http_date(resource_time).expect("resource date should format as HTTP date");

        let mut newer_headers = HeaderMap::new();
        newer_headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_str(
                format_http_date(UNIX_EPOCH + std::time::Duration::from_secs(20))
                    .expect("newer header date should format as HTTP date")
                    .as_str(),
            )
            .expect("if-modified-since header should be valid"),
        );
        assert!(if_modified_since_matches(
            &newer_headers,
            expected_last_modified.as_str(),
        ));

        let mut older_headers = HeaderMap::new();
        older_headers.insert(
            header::IF_MODIFIED_SINCE,
            HeaderValue::from_str(
                format_http_date(UNIX_EPOCH + std::time::Duration::from_secs(5))
                    .expect("older header date should format as HTTP date")
                    .as_str(),
            )
            .expect("if-modified-since header should be valid"),
        );
        assert!(!if_modified_since_matches(
            &older_headers,
            expected_last_modified.as_str(),
        ));
    }

    #[test]
    fn resolve_book_page_bytes_does_not_use_whole_archive_for_non_image() {
        let file_path = unique_temp_path("komga-media-archive");
        fs::write(&file_path, b"archive-bytes").expect("archive test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 12,
        };
        let page = PersistedBookPageRow {
            number: 5,
            file_name: "page-005.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 5);
        assert!(bytes.is_none());

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn resolve_book_page_bytes_allows_single_image_first_page() {
        let file_path = unique_temp_path("komga-media-image");
        fs::write(&file_path, b"image-bytes").expect("image test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "cover.jpg".to_string(),
            file_path: file_path.clone(),
            media_type: "image/jpeg".to_string(),
            page_count: 1,
        };
        let page = PersistedBookPageRow {
            number: 1,
            file_name: "missing-derived-page.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 1);
        assert_eq!(bytes, Some(b"image-bytes".to_vec()));

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn load_archive_page_rows_uses_zip_image_entries_only() {
        let file_path = unique_temp_path("komga-media-zip-rows");
        let archive = build_stored_zip_archive(vec![
            ("001.jpg".to_string(), b"page-1".to_vec()),
            ("meta.txt".to_string(), b"meta".to_vec()),
            ("002.png".to_string(), b"page-2".to_vec()),
        ])
        .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("zip test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 2,
        };

        let rows = load_archive_page_rows(&media).expect("archive rows should be parsed");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].number, 1);
        assert_eq!(rows[0].file_name, "001.jpg");
        assert_eq!(rows[1].number, 2);
        assert_eq!(rows[1].file_name, "002.png");

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn resolve_book_page_bytes_extracts_zip_page_by_logical_index() {
        let file_path = unique_temp_path("komga-media-zip-by-index");
        let archive = build_stored_zip_archive(vec![
            ("001.jpg".to_string(), b"page-1".to_vec()),
            ("meta.txt".to_string(), b"meta".to_vec()),
            ("002.png".to_string(), b"page-2".to_vec()),
        ])
        .expect("zip payload should be created");
        fs::write(&file_path, archive).expect("zip test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.cbz".to_string(),
            file_path: file_path.clone(),
            media_type: "application/vnd.comicbook+zip".to_string(),
            page_count: 2,
        };
        let page = PersistedBookPageRow {
            number: 2,
            file_name: "not-present.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            file_size: 0,
        };

        let bytes = resolve_book_page_bytes(&media, &page, 2);
        assert_eq!(bytes, Some(b"page-2".to_vec()));

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn load_epub_archive_positions_from_file_parses_spine() {
        let file_path = unique_temp_path("komga-media-epub-archive");
        let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let package_document = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="chap-1" href="chapter-1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap-2" href="chapter-2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap-1"/>
    <itemref idref="chap-2"/>
  </spine>
</package>"#;

        let archive = build_stored_zip_archive(vec![
            (
                "META-INF/container.xml".to_string(),
                container_xml.as_bytes().to_vec(),
            ),
            (
                "OEBPS/content.opf".to_string(),
                package_document.as_bytes().to_vec(),
            ),
            (
                "OEBPS/chapter-1.xhtml".to_string(),
                b"<html></html>".to_vec(),
            ),
            (
                "OEBPS/chapter-2.xhtml".to_string(),
                b"<html></html>".to_vec(),
            ),
        ])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.epub".to_string(),
            file_path: file_path.clone(),
            media_type: "application/epub+zip".to_string(),
            page_count: 0,
        };

        let positions =
            load_epub_archive_positions_from_file(&media).expect("epub positions expected");
        assert_eq!(positions.len(), 2);
        assert_eq!(
            positions[0].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[1].get("href"),
            Some(&Value::String("/OEBPS/chapter-2.xhtml".to_string()))
        );
        assert!(positions[0].get("title").is_none());
        assert_eq!(
            positions[0].get("koboSpan"),
            Some(&Value::String("kobo.1.1".to_string()))
        );
        assert_f64_close(
            positions[0]
                .get("locations")
                .and_then(|value| value.get("progression"))
                .and_then(Value::as_f64)
                .expect("progression should be present"),
            0.0,
        );
        assert_f64_close(
            positions[0]
                .get("locations")
                .and_then(|value| value.get("totalProgression"))
                .and_then(Value::as_f64)
                .expect("totalProgression should be present"),
            0.5,
        );
        assert_f64_close(
            positions[1]
                .get("locations")
                .and_then(|value| value.get("totalProgression"))
                .and_then(Value::as_f64)
                .expect("totalProgression should be present"),
            1.0,
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn load_epub_archive_positions_from_file_uses_readium_style_1024_byte_segmentation() {
        let file_path = unique_temp_path("komga-media-epub-segmentation");
        let container_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;
        let package_document = r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf">
  <manifest>
    <item id="chap-1" href="chapter-1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap-2" href="chapter-2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap-1"/>
    <itemref idref="chap-2"/>
  </spine>
</package>"#;

        let chapter_one = vec![b'a'; 2500];
        let chapter_two = vec![b'b'; 100];
        let archive = build_stored_zip_archive(vec![
            (
                "META-INF/container.xml".to_string(),
                container_xml.as_bytes().to_vec(),
            ),
            (
                "OEBPS/content.opf".to_string(),
                package_document.as_bytes().to_vec(),
            ),
            ("OEBPS/chapter-1.xhtml".to_string(), chapter_one),
            ("OEBPS/chapter-2.xhtml".to_string(), chapter_two),
        ])
        .expect("epub archive should be created");
        fs::write(&file_path, archive).expect("epub test file should be written");

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.epub".to_string(),
            file_path: file_path.clone(),
            media_type: "application/epub+zip".to_string(),
            page_count: 0,
        };

        let positions =
            load_epub_archive_positions_from_file(&media).expect("epub positions expected");
        assert_eq!(positions.len(), 4);
        assert_eq!(
            positions[0].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[1].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[2].get("href"),
            Some(&Value::String("/OEBPS/chapter-1.xhtml".to_string()))
        );
        assert_eq!(
            positions[3].get("href"),
            Some(&Value::String("/OEBPS/chapter-2.xhtml".to_string()))
        );

        assert_f64_close(
            positions[1]
                .get("locations")
                .and_then(|value| value.get("progression"))
                .and_then(Value::as_f64)
                .expect("progression should be present"),
            1.0 / 3.0,
        );
        assert_f64_close(
            positions[2]
                .get("locations")
                .and_then(|value| value.get("progression"))
                .and_then(Value::as_f64)
                .expect("progression should be present"),
            2.0 / 3.0,
        );
        assert!(positions[1].get("koboSpan").is_none());
        assert_eq!(
            positions[3].get("koboSpan"),
            Some(&Value::String("kobo.1.1".to_string()))
        );

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn parse_epub_fixed_layout_detects_property_and_name_variants() {
        let by_property = br#"<package><metadata><meta property="rendition:layout">pre-paginated</meta></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_property));

        let by_name =
            br#"<package><metadata><meta name="fixed-layout" content="true"/></metadata></package>"#;
        assert!(parse_epub_fixed_layout(by_name));

        let flowing = br#"<package><metadata><meta property="rendition:layout">reflowable</meta></metadata></package>"#;
        assert!(!parse_epub_fixed_layout(flowing));
    }

    #[test]
    fn parse_epub_kobo_spans_extracts_kobospan_ids_only() {
        let html = br#"<html><body><span class="koboSpan" id="kobo.1.1"></span><span id="kobo.9.9"></span><span class="koboSpan" id="kobo.1.2"></span></body></html>"#;
        let spans = parse_epub_kobo_spans(html);

        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].0, "kobo.1.1");
        assert_eq!(spans[1].0, "kobo.1.2");
    }

    #[test]
    fn normalize_epub_resource_href_collapses_parent_segments() {
        assert_eq!(
            normalize_epub_resource_href("/OPS/sub/content.opf", "../chapter.xhtml"),
            "/OPS/chapter.xhtml"
        );
        assert_eq!(
            normalize_epub_resource_href("/OPS/content.opf", "./text/../chapter.xhtml"),
            "/OPS/chapter.xhtml"
        );
    }

    #[test]
    fn generated_pdf_rows_use_detected_page_count_when_media_count_missing() {
        let file_path = unique_temp_path("komga-media-pdf-archive");
        write_single_page_pdf(&file_path);

        let media = PersistedBookMedia {
            library_id: "lib".to_string(),
            file_name: "book.pdf".to_string(),
            file_path: file_path.clone(),
            media_type: "application/pdf".to_string(),
            page_count: 0,
        };

        let rows = load_generated_pdf_page_rows(&media);
        assert_eq!(rows.len(), 1);
        let bytes = read_pdf_page_as_single_page_pdf(&media, 1);
        assert!(bytes.is_some());

        let _ = fs::remove_file(file_path);
    }

    #[test]
    fn decode_epub_positions_blob_returns_positions_array() {
        let payload = json!({
            "positions": [
                {
                    "href": "/chap-1.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": { "position": 1, "progression": 0.1 }
                },
                {
                    "href": "/chap-2.xhtml",
                    "type": "application/xhtml+xml",
                    "locations": { "position": 2, "progression": 0.2 }
                }
            ]
        });
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(payload.to_string().as_bytes())
            .expect("gzip payload should be writable");
        let blob = encoder.finish().expect("gzip payload should finalize");

        let positions = epub_positions::decode_epub_positions_blob(&blob)
            .expect("epub positions should decode");
        assert_eq!(positions.len(), 2);
        assert_eq!(
            positions[0].get("href"),
            Some(&Value::String("/chap-1.xhtml".to_string()))
        );
    }
}
