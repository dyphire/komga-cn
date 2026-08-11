mod detection;
mod epub;
mod metadata;
mod pdf;

use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use komga_domain::discovery::MediaStatus;
use sqlx::{Row, SqlitePool};
use zip::ZipArchive;

use crate::filesystem::media_analysis::{
    AnalyzedMediaPage, MediaAnalysisProfile, MediaFileAnalysis, MediaFileAnalyzer,
};
use crate::rar_support::read_rar_entry_bytes;
use crate::resolve_stored_path;

use detection::is_recognized_transient_book_file;
pub(crate) use detection::{transient_book_content_type, transient_book_media_type};

const EPUB_DIVINA_LETTER_COUNT_THRESHOLD: usize = 15;
#[derive(Clone, Debug)]
pub(crate) struct TransientBookFileMetadata {
    pub(crate) file_last_modified_unix_nanos: i128,
    pub(crate) size_bytes: u64,
}

#[derive(Clone, Debug)]
pub(crate) struct TransientBookSeriesInference {
    pub(crate) series_id: Option<String>,
    pub(crate) number: Option<f64>,
}

#[derive(Clone, Debug)]
pub(crate) struct TransientBookAnalysis {
    pub(crate) status: MediaStatus,
    pub(crate) media_type: String,
    pub(crate) page_count: u32,
    pub(crate) pages: Vec<TransientBookPage>,
    pub(crate) files: Vec<String>,
    pub(crate) comment: String,
    pub(crate) number: Option<f64>,
    pub(crate) series_id: Option<String>,
}

struct TransientBookMediaAnalysis {
    pages: Vec<TransientBookPage>,
    files: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct TransientBookPage {
    pub(crate) number: u32,
    pub(crate) file_name: String,
    pub(crate) media_type: String,
    pub(crate) width: Option<u32>,
    pub(crate) height: Option<u32>,
    pub(crate) size_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TransientBookPageContent {
    pub(crate) content_type: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TransientBookScanEntry {
    pub(crate) path: String,
    pub(crate) name: String,
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

pub(crate) async fn infer_transient_series_and_number(
    pool: &SqlitePool,
    path_or_name: &str,
) -> Result<TransientBookSeriesInference, String> {
    let inferred = metadata::infer_transient_metadata(path_or_name)?;
    let number = inferred.number;
    if inferred.series_titles.is_empty() {
        return Ok(TransientBookSeriesInference {
            series_id: None,
            number,
        });
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
        .map_err(|error| {
            format!("transient series exact match query failed for '{series_title}': {error}")
        })?
        .map(|row| {
            row.try_get::<String, _>("ID").map_err(|error| {
                format!(
                    "transient series exact match ID decode failed for '{series_title}': {error}"
                )
            })
        })
        .transpose()?;
        if let Some(series_id) = exact_match {
            return Ok(TransientBookSeriesInference {
                series_id: Some(series_id),
                number,
            });
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
        .map_err(|error| {
            format!("transient series fuzzy match query failed for '{series_title}': {error}")
        })?
        .map(|row| {
            row.try_get::<String, _>("ID").map_err(|error| {
                format!(
                    "transient series fuzzy match ID decode failed for '{series_title}': {error}"
                )
            })
        })
        .transpose()?;
        if let Some(series_id) = fuzzy_match {
            return Ok(TransientBookSeriesInference {
                series_id: Some(series_id),
                number,
            });
        }
    }

    Ok(TransientBookSeriesInference {
        series_id: None,
        number,
    })
}

pub(crate) async fn validate_transient_scan_root(
    pool: &SqlitePool,
    root: &Path,
) -> Result<(), String> {
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

    let metadata = fs::metadata(root).map_err(|error| match error.kind() {
        ErrorKind::NotFound => "ERR_1016".to_string(),
        _ => format!(
            "read transient scan root metadata '{}': {error}",
            root.display()
        ),
    })?;
    if !metadata.is_dir() {
        return Err("ERR_1016".to_string());
    }

    if let Err(error) = fs::read_dir(root) {
        if matches!(error.kind(), ErrorKind::NotFound | ErrorKind::NotADirectory) {
            return Err("ERR_1016".to_string());
        }
        return Err(format!(
            "read transient scan root '{}': {error}",
            root.display()
        ));
    }

    Ok(())
}

pub(crate) fn transient_book_exists(path: &str) -> Result<bool, String> {
    Path::new(path)
        .try_exists()
        .map_err(|error| format!("check transient book existence '{path}': {error}"))
}

pub(crate) fn load_transient_book_file_metadata(
    path: &str,
) -> Result<TransientBookFileMetadata, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("read transient book metadata '{path}': {error}"))?;
    Ok(TransientBookFileMetadata {
        file_last_modified_unix_nanos: to_unix_nanos(newest_file_system_time(
            metadata.created().ok(),
            metadata.modified().ok(),
        )),
        size_bytes: metadata.len(),
    })
}

pub(crate) fn load_transient_book_media(path: &str) -> Result<Vec<u8>, String> {
    fs::read(path).map_err(|error| format!("read transient book media '{path}': {error}"))
}

pub(crate) fn analyze_transient_book(path: &str) -> Result<TransientBookAnalysis, String> {
    if !transient_book_exists(path)? {
        return Ok(transient_analysis_error(
            MediaStatus::Error,
            String::new(),
            "ERR_1018",
        ));
    }

    let media_type = transient_book_media_type(path);
    if PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("epub"))
        && media_type != "application/epub+zip"
    {
        return Ok(transient_analysis_error(
            MediaStatus::Error,
            media_type,
            "ERR_1032",
        ));
    }

    let analysis_result = if media_type.starts_with("image/") {
        analyze_transient_media_file(path).map_err(|_| "ERR_1001")
    } else if media_type == "application/epub+zip" {
        return Ok(
            epub::analyze_transient_epub(path).unwrap_or_else(|error_code| {
                transient_analysis_error(MediaStatus::Error, media_type, error_code)
            }),
        );
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
        return Ok(transient_analysis_error(
            MediaStatus::Unsupported,
            media_type,
            "ERR_1001",
        ));
    };

    let analysis = match analysis_result {
        Ok(result) => result,
        Err(error_code) => {
            return Ok(transient_analysis_error(
                MediaStatus::Error,
                media_type,
                error_code,
            ));
        }
    };

    if analysis.pages.is_empty() {
        return Ok(transient_analysis_error(
            MediaStatus::Error,
            media_type,
            "ERR_1006",
        ));
    }

    Ok(TransientBookAnalysis {
        status: MediaStatus::Ready,
        media_type,
        page_count: analysis.pages.len() as u32,
        pages: analysis.pages,
        files: analysis.files,
        comment: String::new(),
        number: None,
        series_id: None,
    })
}

fn analyze_transient_media_file(path: &str) -> Result<TransientBookMediaAnalysis, String> {
    let analysis = MediaFileAnalyzer.analyze(Path::new(path), MediaAnalysisProfile::Transient)?;
    Ok(transient_pages_from_media_analysis(analysis))
}

fn transient_pages_from_media_analysis(analysis: MediaFileAnalysis) -> TransientBookMediaAnalysis {
    let media_type = analysis.media_type;
    let pages = analysis
        .pages
        .into_iter()
        .enumerate()
        .map(|(index, page)| transient_page_from_analyzed_media_page(index, page, &media_type))
        .collect();

    TransientBookMediaAnalysis {
        pages,
        files: analysis.files,
    }
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
    status: MediaStatus,
    media_type: String,
    comment: &str,
) -> TransientBookAnalysis {
    TransientBookAnalysis {
        status,
        media_type,
        page_count: 0,
        pages: Vec::new(),
        files: Vec::new(),
        comment: comment.to_string(),
        number: None,
        series_id: None,
    }
}

pub(crate) fn transient_book_page_content(
    path: &str,
    media_type: &str,
    pages: &[TransientBookPage],
    page_number: u32,
) -> Result<Option<TransientBookPageContent>, String> {
    if page_number == 0 {
        return Ok(None);
    }

    let media_type = if media_type.is_empty() {
        transient_book_media_type(path)
    } else {
        media_type.to_string()
    };
    let content_type = transient_book_content_type(path, media_type.as_str());

    if media_type.starts_with("image/") {
        if page_number != 1 {
            return Ok(None);
        }
        let bytes = load_transient_book_media(path)?;
        return Ok(Some(TransientBookPageContent {
            content_type: media_type,
            bytes,
        }));
    }

    let page = pages
        .iter()
        .find(|entry| entry.number == page_number)
        .cloned();
    let Some(page) = page else {
        return Ok(None);
    };

    if matches!(content_type, "application/zip" | "application/epub+zip") {
        let file = fs::File::open(path)
            .map_err(|error| format!("open transient archive '{path}': {error}"))?;
        let mut archive = ZipArchive::new(file)
            .map_err(|error| format!("read transient archive '{path}': {error}"))?;
        let mut entry = archive.by_name(page.file_name.as_str()).map_err(|error| {
            format!(
                "read transient archive entry '{}' from '{}': {error}",
                page.file_name, path
            )
        })?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|error| {
            format!(
                "read transient archive entry '{}' bytes from '{}': {error}",
                page.file_name, path
            )
        })?;
        return Ok(Some(TransientBookPageContent {
            content_type: page.media_type,
            bytes,
        }));
    }

    if matches!(
        content_type,
        "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5"
    ) {
        let bytes =
            read_rar_entry_bytes(Path::new(path), page.file_name.as_str())?.ok_or_else(|| {
                format!(
                    "read transient rar entry '{}' from '{}': entry not found",
                    page.file_name, path
                )
            })?;
        return Ok(Some(TransientBookPageContent {
            content_type: page.media_type,
            bytes,
        }));
    }

    if content_type == "application/pdf" {
        let bytes = pdf::render_pdf_page_image_bytes(path, page_number)?;
        return Ok(Some(TransientBookPageContent {
            content_type: "image/jpeg".to_string(),
            bytes,
        }));
    }

    Ok(None)
}

pub(crate) fn list_transient_book_entries(
    path: &Path,
) -> Result<Vec<TransientBookScanEntry>, String> {
    let mut entries = Vec::new();
    collect_transient_book_entries(path, &mut entries)?;
    entries.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(entries)
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

fn collect_transient_book_entries(
    path: &Path,
    entries: &mut Vec<TransientBookScanEntry>,
) -> Result<(), String> {
    let directory_entries = fs::read_dir(path)
        .map_err(|error| format!("read transient directory '{}': {error}", path.display()))?;

    for entry in directory_entries {
        let entry = entry.map_err(|error| {
            format!(
                "read transient directory entry from '{}': {error}",
                path.display()
            )
        })?;
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "read transient directory entry type '{}': {error}",
                entry.path().display()
            )
        })?;

        let entry_path = entry.path();
        let is_hidden = entry_path
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|name| name.starts_with('.'));
        if is_hidden {
            continue;
        }

        if file_type.is_dir() {
            collect_transient_book_entries(&entry_path, entries)?;
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

        entries.push(TransientBookScanEntry {
            path: entry_path.to_string_lossy().to_string(),
            name,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::BootstrappedBookFixture;
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

    fn write_epub_with_package_and_entries(
        path: &Path,
        package_document: &str,
        entries: &[(&str, &[u8])],
    ) {
        let file = File::create(path).expect("epub fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);

        zip.start_file("mimetype", options)
            .expect("epub mimetype entry should be created");
        zip.write_all(b"application/epub+zip")
            .expect("epub mimetype bytes should be written");
        zip.start_file("META-INF/container.xml", options)
            .expect("epub container entry should be created");
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#,
        )
        .expect("epub container bytes should be written");
        zip.start_file("OEBPS/content.opf", options)
            .expect("epub package entry should be created");
        zip.write_all(package_document.as_bytes())
            .expect("epub package bytes should be written");
        for (entry_path, bytes) in entries {
            zip.start_file(*entry_path, options)
                .expect("epub extra entry should be created");
            zip.write_all(bytes)
                .expect("epub extra entry bytes should be written");
        }

        zip.finish()
            .expect("epub fixture should finish successfully");
    }

    fn write_epub_with_package(path: &Path, package_document: &str) {
        write_epub_with_package_and_entries(path, package_document, &[]);
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

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("transient image analysis should complete");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(3));
        assert_eq!(analysis.pages[0].height, Some(5));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_returns_error_for_unreadable_single_image() {
        let path = unique_temp_path("unreadable-single-image", "png");
        fs::create_dir(&path).expect("unreadable single image fixture should be a directory");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("unreadable transient image should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.comment, "ERR_1001");
        assert!(analysis.pages.is_empty());

        let _ = fs::remove_dir(path);
    }

    #[test]
    fn analyze_transient_book_returns_err_1001_for_invalid_single_image_bytes() {
        let path = unique_temp_path("invalid-single-image", "png");
        fs::write(&path, b"not-an-image").expect("invalid single image fixture should be written");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("invalid transient image should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "image/png");
        assert_eq!(analysis.comment, "ERR_1001");
        assert!(analysis.pages.is_empty());

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

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("transient cbz analysis should complete");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(7));
        assert_eq!(analysis.pages[0].height, Some(11));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_returns_err_1008_for_invalid_cbz_page_image_bytes() {
        let path = unique_temp_path("invalid-cbz-page-image", "cbz");
        let file = File::create(&path).expect("cbz fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("page-1.png", options)
            .expect("cbz page entry should be created");
        zip.write_all(b"not-an-image")
            .expect("cbz invalid page bytes should be written");
        zip.finish()
            .expect("cbz fixture should finish successfully");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("invalid transient cbz should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "application/zip");
        assert_eq!(analysis.comment, "ERR_1008");
        assert!(analysis.pages.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_pdf_page_dimensions() {
        let path = unique_temp_path("pdf-page-dimensions", "pdf");
        write_single_page_pdf(&path, 595, 842);

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("transient pdf analysis should complete");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].width, Some(3200));
        assert_eq!(analysis.pages[0].height, Some(4528));

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_populates_rar_page_dimensions() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("transient rar analysis should complete");

        assert_eq!(analysis.status, MediaStatus::Ready);
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

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("transient rar analysis should complete");

        assert_eq!(analysis.status, MediaStatus::Ready);
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

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("missing transient book should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "");
        assert_eq!(analysis.comment, "ERR_1018");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn analyze_transient_book_propagates_existence_probe_errors() {
        let parent_file = unique_temp_path("probe-parent-file", "tmp");
        fs::write(&parent_file, b"not a directory").expect("parent file fixture should be written");
        let path = parent_file.join("book.png");

        let error = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect_err("filesystem probe error should fail transient analysis");

        assert!(
            error.contains("check transient book existence"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(parent_file);
    }

    #[test]
    fn analyze_transient_book_returns_err_1032_for_broken_epub() {
        let path = unique_temp_path("broken-epub", "epub");
        write_zip_as_epub(&path);

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("broken transient epub should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "application/zip");
        assert_eq!(analysis.comment, "ERR_1032");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_returns_err_1032_for_malformed_epub_package() {
        let path = unique_temp_path("malformed-epub-package", "epub");
        write_epub_with_package(
            &path,
            r#"<package><manifest><item id= href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="main"/></spine></package>"#,
        );

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("malformed transient epub should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "application/epub+zip");
        assert_eq!(analysis.comment, "ERR_1032");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_returns_err_1032_for_malformed_epub_spine_resource() {
        let path = unique_temp_path("malformed-epub-spine-resource", "epub");
        write_epub_with_package_and_entries(
            &path,
            r#"<package><manifest><item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/><item id="image" href="cover.png" media-type="image/png"/></manifest><spine><itemref idref="main"/></spine></package>"#,
            &[(
                "OEBPS/chapter.xhtml",
                br#"<html><body><img src="cover.png"></body"#,
            )],
        );

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("malformed transient epub spine resource should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "application/epub+zip");
        assert_eq!(analysis.comment, "ERR_1032");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_returns_err_1032_for_invalid_epub_image_spine_resource() {
        let path = unique_temp_path("invalid-epub-image-spine-resource", "epub");
        write_epub_with_package_and_entries(
            &path,
            r#"<package><manifest><item id="page" href="page.png" media-type="image/png"/></manifest><spine><itemref idref="page"/></spine></package>"#,
            &[("OEBPS/page.png", b"not-an-image")],
        );

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("invalid transient epub image resource should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "application/epub+zip");
        assert_eq!(analysis.comment, "ERR_1032");
        assert!(analysis.pages.is_empty());
        assert!(analysis.files.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn analyze_transient_book_detects_svg_wrapped_epub_image_as_divina() {
        let path = unique_temp_path("svg-wrapped-divina-image", "epub");
        let image = png_bytes(2, 3);
        let page = br#"<html xmlns="http://www.w3.org/1999/xhtml"><body><svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="page.png"/></svg></body></html>"#;
        assert_eq!(
            epub::parse_transient_epub_divina_image_href(page, "OEBPS/page.xhtml")
                .expect("SVG spine page should parse"),
            Some("OEBPS/page.png".to_string())
        );
        write_epub_with_package_and_entries(
            &path,
            r#"<package><manifest><item id="main" href="page.xhtml" media-type="application/xhtml+xml"/><item id="image" href="page.png" media-type="image/png"/></manifest><spine><itemref idref="main"/></spine></package>"#,
            &[
                ("OEBPS/page.xhtml", page),
                ("OEBPS/page.png", image.as_slice()),
            ],
        );

        let analysis = analyze_transient_book(path.to_string_lossy().as_ref())
            .expect("SVG-wrapped EPUB should produce an analysis result");

        assert_eq!(analysis.status, MediaStatus::Ready, "{}", analysis.comment);
        assert_eq!(analysis.pages.len(), 1);
        assert_eq!(analysis.pages[0].file_name, "OEBPS/page.png");

        let _ = fs::remove_file(path);
    }

    #[tokio::test]
    async fn infer_transient_series_and_number_propagates_series_lookup_errors() {
        let fixture = BootstrappedBookFixture::open("transient-infer-series-query-error").await;
        let path = unique_temp_path("infer-series-query-error", "cbz");
        let file = File::create(&path).expect("cbz metadata fixture should be created");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .unix_permissions(0o644);
        zip.start_file("ComicInfo.xml", options)
            .expect("comicinfo entry should be created");
        zip.write_all(b"<ComicInfo><Series>Series 1</Series><Number>7</Number></ComicInfo>")
            .expect("comicinfo entry should be written");
        zip.finish()
            .expect("cbz metadata fixture should finish successfully");
        sqlx::query("DROP TABLE SERIES_METADATA")
            .execute(&fixture.pool)
            .await
            .expect("series metadata table should be dropped for corrupt schema fixture");

        let error =
            infer_transient_series_and_number(&fixture.pool, path.to_string_lossy().as_ref())
                .await
                .expect_err("series lookup query errors must not become an empty inference");

        assert!(
            error.contains("transient series exact match query failed"),
            "unexpected inference error: {error}"
        );

        let _ = fs::remove_file(path);
        fixture.close().await;
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn infer_transient_series_and_number_propagates_metadata_source_errors() {
        let root = unique_temp_path("infer-metadata-source-error-root", "tmp");
        fs::create_dir(&root).expect("metadata source fixture root should be created");
        let path = root.join("book.cbz");
        std::os::unix::fs::symlink(&path, &path)
            .expect("metadata source symlink loop should be created");
        let pool = sqlx::SqlitePool::connect_lazy(":memory:")
            .expect("lazy in-memory pool should not fail");

        let error = infer_transient_series_and_number(&pool, path.to_string_lossy().as_ref())
            .await
            .expect_err("metadata source errors must not become empty inference");

        assert!(
            error.contains("open transient metadata archive"),
            "unexpected metadata source error: {error}"
        );

        let _ = fs::remove_file(path);
        let _ = fs::remove_dir(root);
    }

    #[tokio::test]
    async fn infer_transient_series_and_number_propagates_epub_metadata_parse_errors() {
        let path = unique_temp_path("infer-epub-metadata-parse-error", "epub");
        write_epub_with_package(
            &path,
            r#"<package><metadata><dc:creator id= role="aut">Jane</dc:creator></metadata><manifest><item id="main" href="chapter.xhtml" media-type="application/xhtml+xml"/></manifest></package>"#,
        );
        let pool = sqlx::SqlitePool::connect_lazy(":memory:")
            .expect("lazy in-memory pool should not fail");

        let error = infer_transient_series_and_number(&pool, path.to_string_lossy().as_ref())
            .await
            .expect_err("EPUB provider metadata parse errors must not become empty inference");

        assert!(
            error.contains("EPUB package document"),
            "unexpected metadata parse error: {error}"
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn list_transient_book_entries_propagates_read_dir_errors() {
        let path = unique_temp_path("list-root-file", "cbz");
        fs::write(&path, b"not-a-directory").expect("file fixture should be written");

        let error = list_transient_book_entries(&path)
            .expect_err("read_dir errors must not become an empty transient scan");

        assert!(
            error.contains("read transient directory"),
            "unexpected list error: {error}"
        );

        let _ = fs::remove_file(path);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn validate_transient_scan_root_propagates_filesystem_probe_errors() {
        let fixture = BootstrappedBookFixture::open("transient-scan-root-probe-error").await;
        let path = unique_temp_path("scan-root-probe-loop", "tmp");
        std::os::unix::fs::symlink(&path, &path).expect("scan root symlink loop should be created");

        let error = validate_transient_scan_root(&fixture.pool, &path)
            .await
            .expect_err("scan root filesystem probe errors must not become ERR_1016");

        assert_ne!(error, "ERR_1016");

        let _ = fs::remove_file(path);
        fixture.close().await;
    }

    #[test]
    fn transient_book_page_content_propagates_image_read_errors() {
        let path = unique_temp_path("image-page-read-error", "jpg");
        fs::create_dir(&path).expect("image read error fixture should be a directory");

        let error =
            transient_book_page_content(path.to_string_lossy().as_ref(), "image/jpeg", &[], 1)
                .expect_err("image read errors must not become a missing page");

        assert!(
            error.contains("read transient book media"),
            "unexpected page content error: {error}"
        );

        let _ = fs::remove_dir(path);
    }

    #[test]
    fn transient_book_page_content_propagates_missing_archive_entries() {
        let path = unique_temp_path("missing-archive-entry", "cbz");
        let file = File::create(&path).expect("cbz fixture should be created");
        ZipWriter::new(file)
            .finish()
            .expect("empty cbz fixture should finish successfully");
        let pages = vec![TransientBookPage {
            number: 1,
            file_name: "missing.jpg".to_string(),
            media_type: "image/jpeg".to_string(),
            width: None,
            height: None,
            size_bytes: None,
        }];

        let error = transient_book_page_content(
            path.to_string_lossy().as_ref(),
            "application/zip",
            &pages,
            1,
        )
        .expect_err("archive entry read errors must not become a missing page");

        assert!(
            error.contains("read transient archive entry"),
            "unexpected page content error: {error}"
        );

        let _ = fs::remove_file(path);
    }
}
