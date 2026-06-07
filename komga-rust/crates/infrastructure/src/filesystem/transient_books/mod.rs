mod detection;
mod epub;
mod metadata;
mod pdf;

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};
use zip::ZipArchive;

use crate::filesystem::media_analysis::{
    AnalyzedMediaPage, MediaAnalysisProfile, MediaFileAnalysis, MediaFileAnalyzer,
};
use crate::rar_support::read_rar_entry_bytes;
use crate::resolve_stored_path;

use detection::is_recognized_transient_book_file;
pub use detection::{transient_book_content_type, transient_book_media_type};

const EPUB_DIVINA_LETTER_COUNT_THRESHOLD: usize = 15;
#[derive(Clone, Debug)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

#[derive(Clone, Debug)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
    pub page_count: u32,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[derive(Default)]
struct TransientMetadataInference {
    series_titles: Vec<String>,
    number: Option<f64>,
}

#[derive(Clone)]
struct TransientEpubManifestItem {
    href: String,
    media_type: String,
}

pub async fn infer_transient_series_and_number(
    pool: &SqlitePool,
    path_or_name: &str,
) -> (Option<String>, Option<f64>) {
    let inferred = metadata::infer_transient_metadata(path_or_name);
    let number = inferred.number;
    if inferred.series_titles.is_empty() {
        return (None, number);
    }

    for series_title in &inferred.series_titles {
        let exact_match = sqlx::query(
            r#"SELECT s.ID AS ID
             FROM SERIES s
             LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) = LOWER(?)
             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC
             LIMIT 1"#,
        )
        .bind(series_title.as_str())
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("ID"));
        if let Some(series_id) = exact_match {
            return (Some(series_id), number);
        }
    }

    for series_title in &inferred.series_titles {
        let fuzzy_match = sqlx::query(
            r#"SELECT s.ID AS ID
             FROM SERIES s
             LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID
             WHERE LOWER(COALESCE(sm.TITLE, s.NAME)) LIKE LOWER(?)
             ORDER BY s.LAST_MODIFIED_DATE DESC, s.ID ASC
             LIMIT 1"#,
        )
        .bind(format!("%{}%", series_title))
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .map(|row| row.get::<String, _>("ID"));
        if let Some(series_id) = fuzzy_match {
            return (Some(series_id), number);
        }
    }

    (None, number)
}

pub async fn validate_transient_scan_root(pool: &SqlitePool, root: &Path) -> Result<(), String> {
    let library_roots = sqlx::query("SELECT ROOT AS ROOT FROM LIBRARY")
        .fetch_all(pool)
        .await
        .map_err(|error| format!("transient scan library roots query failed: {error}"))?
        .into_iter()
        .map(|row| resolve_stored_path(row.get::<String, _>("ROOT").as_str()))
        .collect::<Vec<_>>();

    for library_root in library_roots {
        if root.starts_with(&library_root) {
            return Err("ERR_1017".to_string());
        }
    }

    if !root.is_dir() || fs::read_dir(root).is_err() {
        return Err("ERR_1016".to_string());
    }

    Ok(())
}

pub fn transient_book_exists(path: &str) -> bool {
    Path::new(path).exists()
}

pub fn load_transient_book_file_metadata(path: &str) -> Option<TransientBookFileMetadata> {
    let metadata = fs::metadata(path).ok()?;
    Some(TransientBookFileMetadata {
        file_last_modified_unix_nanos: to_unix_nanos(newest_file_system_time(
            metadata.created().ok(),
            metadata.modified().ok(),
        )),
        size_bytes: metadata.len(),
    })
}

pub fn load_transient_book_media(path: &str) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

pub fn analyze_transient_book(path: &str) -> TransientBookAnalysis {
    if !transient_book_exists(path) {
        return transient_analysis_error("ERROR", String::new(), "ERR_1018");
    }

    let media_type = transient_book_media_type(path);
    if PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
        && media_type != "application/epub+zip"
    {
        return transient_analysis_error("ERROR", media_type, "ERR_1032");
    }

    let analysis_result = if media_type.starts_with("image/") {
        analyze_transient_media_file(path).map_err(|_| "ERR_1001")
    } else if media_type == "application/epub+zip" {
        return epub::analyze_transient_epub(path).unwrap_or_else(|error_code| {
            transient_analysis_error("ERROR", media_type, error_code)
        });
    } else if media_type == "application/zip" {
        analyze_transient_media_file(path).map_err(|_| "ERR_1008")
    } else if matches!(
        media_type.as_str(),
        "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) {
        analyze_transient_media_file(path).map_err(|_| "ERR_1008")
    } else if media_type == "application/pdf" {
        analyze_transient_media_file(path).map_err(|_| "ERR_1005")
    } else {
        return transient_analysis_error("UNSUPPORTED", media_type, "ERR_1001");
    };

    let (pages, files) = match analysis_result {
        Ok(result) => result,
        Err(error_code) => return transient_analysis_error("ERROR", media_type, error_code),
    };

    if pages.is_empty() {
        return transient_analysis_error("ERROR", media_type, "ERR_1006");
    }

    TransientBookAnalysis {
        status: "READY".to_string(),
        media_type,
        page_count: pages.len() as u32,
        pages,
        files,
        comment: String::new(),
        number: None,
        series_id: None,
    }
}

fn analyze_transient_media_file(
    path: &str,
) -> Result<(Vec<TransientBookPage>, Vec<String>), String> {
    let analysis = MediaFileAnalyzer.analyze(Path::new(path), MediaAnalysisProfile::Transient)?;
    Ok(transient_pages_from_media_analysis(analysis))
}

fn transient_pages_from_media_analysis(
    analysis: MediaFileAnalysis,
) -> (Vec<TransientBookPage>, Vec<String>) {
    let media_type = analysis.media_type;
    let pages = analysis
        .pages
        .into_iter()
        .enumerate()
        .map(|(index, page)| transient_page_from_analyzed_media_page(index, page, &media_type))
        .collect();

    (pages, analysis.files)
}

fn transient_page_from_analyzed_media_page(
    index: usize,
    page: AnalyzedMediaPage,
    media_type: &str,
) -> TransientBookPage {
    TransientBookPage {
        number: (index as u32) + 1,
        file_name: page.file_name,
        media_type: page.media_type,
        width: page.width.and_then(|width| width.try_into().ok()),
        height: page.height.and_then(|height| height.try_into().ok()),
        size_bytes: transient_page_size_bytes(page.file_size, media_type),
    }
}

fn transient_page_size_bytes(file_size: i64, media_type: &str) -> Option<u64> {
    if media_type == "application/pdf" {
        return None;
    }

    file_size.try_into().ok()
}

fn transient_analysis_error(
    status: &str,
    media_type: String,
    comment: &str,
) -> TransientBookAnalysis {
    TransientBookAnalysis {
        status: status.to_string(),
        media_type,
        page_count: 0,
        pages: Vec::new(),
        files: Vec::new(),
        comment: comment.to_string(),
        number: None,
        series_id: None,
    }
}

pub fn transient_book_page_content(
    path: &str,
    media_type: &str,
    pages: &[TransientBookPage],
    page_number: u32,
) -> Option<(String, Vec<u8>)> {
    if page_number == 0 {
        return None;
    }

    let media_type = if media_type.is_empty() {
        transient_book_media_type(path)
    } else {
        media_type.to_string()
    };
    let content_type = transient_book_content_type(path, media_type.as_str());

    if media_type.starts_with("image/") {
        if page_number != 1 {
            return None;
        }
        let bytes = load_transient_book_media(path)?;
        return Some((media_type, bytes));
    }

    let page = pages
        .iter()
        .find(|entry| entry.number == page_number)
        .cloned()?;

    if matches!(content_type, "application/zip" | "application/epub+zip") {
        let file = fs::File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;
        let mut entry = archive.by_name(page.file_name.as_str()).ok()?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).ok()?;
        return Some((page.media_type, bytes));
    }

    if matches!(
        content_type,
        "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) {
        let bytes = read_rar_entry_bytes(Path::new(path), page.file_name.as_str())
            .ok()
            .flatten()?;
        return Some((page.media_type, bytes));
    }

    if content_type == "application/pdf" {
        let bytes = pdf::render_pdf_page_image_bytes(path, page_number)?;
        return Some(("image/jpeg".to_string(), bytes));
    }

    None
}

pub fn list_transient_book_entries(path: &Path) -> Vec<Value> {
    let mut entries = Vec::new();
    collect_transient_book_entries(path, &mut entries);
    entries.sort_by(|left, right| {
        left["path"]
            .as_str()
            .unwrap_or_default()
            .cmp(right["path"].as_str().unwrap_or_default())
    });
    entries
}

fn to_unix_nanos(time: Option<SystemTime>) -> i128 {
    time.and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos() as i128)
        .unwrap_or_default()
}

fn newest_file_system_time(
    left: Option<SystemTime>,
    right: Option<SystemTime>,
) -> Option<SystemTime> {
    match (left, right) {
        (Some(left), Some(right)) => Some(std::cmp::max(left, right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn collect_transient_book_entries(path: &Path, entries: &mut Vec<Value>) {
    let Ok(directory_entries) = fs::read_dir(path) else {
        return;
    };

    for entry in directory_entries.filter_map(Result::ok) {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        let entry_path = entry.path();
        let is_hidden = entry_path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with('.'));
        if is_hidden {
            continue;
        }

        if file_type.is_dir() {
            collect_transient_book_entries(&entry_path, entries);
            continue;
        }

        if !is_recognized_transient_book_file(&entry_path) {
            continue;
        }

        let Some(name) = entry_path
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .filter(|stem| !stem.is_empty())
        else {
            continue;
        };

        entries.push(json!({
            "name": name,
            "path": entry_path.to_string_lossy().to_string(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgba};
    use lopdf::{Document as PdfDocument, Object, Stream, dictionary};
    use std::fs::File;
    use std::io::Write;
    use zip::CompressionMethod;
    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    fn unique_temp_path(case: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("komga-transient-{case}-{nanos}.{extension}"))
    }

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let image =
            ImageBuffer::<Rgba<u8>, Vec<u8>>::from_pixel(width, height, Rgba([1, 2, 3, 255]));
        let mut output = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut output, image::ImageFormat::Png)
            .expect("png fixture should encode");
        output.into_inner()
    }

    fn write_zip_as_epub(path: &Path) {
        let file = File::create(path).expect("zip-as-epub fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("zip-as-epub page entry should be created");
        zip.write_all(b"not-an-image")
            .expect("zip-as-epub page bytes should be written");
        zip.finish()
            .expect("zip-as-epub fixture should finish successfully");
    }

    fn write_single_page_pdf(path: &Path, width: i64, height: i64) {
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
                "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
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
            .expect("single-page pdf fixture should save");
    }

    #[test]
    fn analyze_transient_book_populates_image_dimensions_for_single_image() {
        let path = unique_temp_path("single-image", "png");
        fs::write(&path, png_bytes(3, 5)).expect("transient image fixture should be written");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(3));
        assert_eq!(analysis.pages[0].height, Some(5));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_image_dimensions_for_cbz_pages() {
        let path = unique_temp_path("cbz-image-dimensions", "cbz");
        let file = File::create(&path).expect("cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("cbz page entry should be created");
        zip.write_all(&png_bytes(7, 11))
            .expect("cbz page bytes should be written");
        zip.finish()
            .expect("cbz fixture should finish successfully");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(7));
        assert_eq!(analysis.pages[0].height, Some(11));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_pdf_page_dimensions() {
        let path = unique_temp_path("pdf-page-dimensions", "pdf");
        write_single_page_pdf(&path, 595, 842);

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(3200));
        assert_eq!(analysis.pages[0].height, Some(4528));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_rar_page_dimensions() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert_eq!(
            analysis.media_type,
            "application/x-rar-compressed; version=4"
        );
        assert!(
            !analysis.pages.is_empty(),
            "rar transient pages should not be empty"
        );
        assert!(
            analysis.pages[0].width.is_some(),
            "rar page width should be populated"
        );
        assert!(
            analysis.pages[0].height.is_some(),
            "rar page height should be populated"
        );
    }

    #[test]
    fn analyze_transient_book_populates_rar_page_size_bytes() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "READY");
        assert!(
            !analysis.pages.is_empty(),
            "rar transient pages should not be empty"
        );
        assert!(
            analysis.pages[0].size_bytes.is_some_and(|size| size > 0),
            "rar page size_bytes should be populated"
        );
    }

    #[test]
    fn analyze_transient_book_returns_err_1018_for_missing_file() {
        let path = unique_temp_path("missing-file", "png");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "ERROR");
        assert_eq!(analysis.media_type, "");
        assert_eq!(analysis.comment, "ERR_1018");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());
    }

    #[test]
    fn analyze_transient_book_returns_err_1032_for_broken_epub() {
        let path = unique_temp_path("broken-epub", "epub");
        write_zip_as_epub(&path);

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref());

        assert_eq!(analysis.status, "ERROR");
        assert_eq!(analysis.media_type, "application/zip");
        assert_eq!(analysis.comment, "ERR_1032");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());

        let _ = fs::remove_file(path);
    }
}
