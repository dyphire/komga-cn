use super::*;
use crate::rar_support::{detect_rar_media_type, list_rar_entries, read_rar_entry_bytes};
use image::GenericImageView;
use lopdf::{Document as PdfDocument, Object};
use std::io::Read;
use std::path::Path;

pub(in crate::task_queue) fn media_type_from_path(path: &str) -> String {
    let extension = PathBuf::from(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    match extension.as_str() {
        "cbz" | "zip" => "application/zip",
        "cbr" | "rar" => "application/vnd.comicbook-rar",
        "pdf" => "application/pdf",
        "epub" => "application/epub+zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct AnalyzedMediaPageRow {
    pub(in crate::task_queue) file_name: String,
    pub(in crate::task_queue) media_type: String,
    pub(in crate::task_queue) width: Option<i64>,
    pub(in crate::task_queue) height: Option<i64>,
    pub(in crate::task_queue) file_size: i64,
}

#[derive(Clone, Debug)]
pub(in crate::task_queue) struct BookMediaAnalysis {
    pub(in crate::task_queue) status: String,
    pub(in crate::task_queue) media_type: String,
    pub(in crate::task_queue) pages: Vec<AnalyzedMediaPageRow>,
}

pub(in crate::task_queue) fn analyze_book_media_file(
    file_path: &PathBuf,
    book_url: &str,
    analyze_dimensions: bool,
) -> Result<BookMediaAnalysis, String> {
    let media_type = match media_type_from_path(book_url).as_str() {
        "application/vnd.comicbook-rar" => detect_rar_media_type(file_path).to_string(),
        other => other.to_string(),
    };

    if !file_path.exists() {
        return Ok(BookMediaAnalysis {
            status: "ERROR".to_string(),
            media_type,
            pages: Vec::new(),
        });
    }

    let pages = match media_type.as_str() {
        "application/zip" => {
            analyze_zip_media_pages(file_path, false, analyze_dimensions).unwrap_or_default()
        }
        "application/epub+zip" => {
            analyze_zip_media_pages(file_path, true, analyze_dimensions).unwrap_or_default()
        }
        "application/vnd.comicbook-rar"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => {
            analyze_rar_media_pages(file_path, analyze_dimensions).unwrap_or_default()
        }
        "application/pdf" => {
            analyze_pdf_media_pages(file_path, analyze_dimensions).unwrap_or_default()
        }
        _ => {
            return Ok(BookMediaAnalysis {
                status: "UNSUPPORTED".to_string(),
                media_type,
                pages: Vec::new(),
            });
        }
    };

    let status = if pages.is_empty() { "ERROR" } else { "READY" }.to_string();

    Ok(BookMediaAnalysis {
        status,
        media_type,
        pages,
    })
}

pub(in crate::task_queue) fn analyze_zip_media_pages(
    file_path: &PathBuf,
    include_epub_resources: bool,
    analyze_dimensions: bool,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let file = fs::File::open(file_path)
        .map_err(|error| format!("open zip file '{}': {error}", file_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("open zip archive '{}': {error}", file_path.display()))?;

    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("read zip entry at index {index}: {error}"))?;
        if entry.is_dir() {
            continue;
        }

        let file_name = entry.name().to_string();
        let include = if include_epub_resources {
            is_epub_page_resource_file_name(&file_name)
        } else {
            is_supported_page_image_file_name(&file_name)
        };
        if !include {
            continue;
        }

        let media_type = media_type_from_entry_name(&file_name);
        let dimensions = if analyze_dimensions && media_type.starts_with("image/") {
            let mut bytes = Vec::new();
            entry
                .read_to_end(&mut bytes)
                .map_err(|error| format!("read zip entry bytes for '{file_name}': {error}"))?;
            image_dimensions_from_bytes(&bytes)
        } else {
            None
        };
        let (width, height) = split_dimensions(dimensions);
        let file_size = i64::try_from(entry.size()).unwrap_or(i64::MAX);
        pages.push(AnalyzedMediaPageRow {
            media_type,
            file_name,
            width,
            height,
            file_size,
        });
    }

    Ok(pages)
}

pub(in crate::task_queue) fn analyze_rar_media_pages(
    file_path: &Path,
    analyze_dimensions: bool,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    Ok(list_rar_entries(file_path)?
        .into_iter()
        .filter(|entry| is_supported_page_image_file_name(&entry.file_name))
        .map(|entry| {
            let dimensions = if analyze_dimensions {
                read_rar_entry_bytes(file_path, &entry.file_name)
                    .ok()
                    .flatten()
                    .as_deref()
                    .and_then(image_dimensions_from_bytes)
            } else {
                None
            };
            let (width, height) = split_dimensions(dimensions);

            AnalyzedMediaPageRow {
                media_type: media_type_from_entry_name(&entry.file_name),
                file_name: entry.file_name,
                width,
                height,
                file_size: entry.unpacked_size.try_into().unwrap_or(i64::MAX),
            }
        })
        .collect::<Vec<_>>())
}

pub(in crate::task_queue) fn analyze_pdf_media_pages(
    file_path: &Path,
    analyze_dimensions: bool,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let document = PdfDocument::load(file_path)
        .map_err(|error| format!("load pdf '{}': {error}", file_path.display()))?;
    let page_count = document.get_pages().len();
    Ok((0..page_count)
        .map(|index| {
            let dimensions = analyze_dimensions
                .then(|| pdf_page_dimensions(&document, (index + 1) as u32))
                .flatten();
            let (width, height) = split_dimensions(dimensions);

            AnalyzedMediaPageRow {
                file_name: format!("page-{index:04}.pdf"),
                media_type: "application/pdf".to_string(),
                width,
                height,
                file_size: 0,
            }
        })
        .collect::<Vec<_>>())
}

fn image_dimensions_from_bytes(bytes: &[u8]) -> Option<(i64, i64)> {
    let image = image::load_from_memory(bytes).ok()?;
    let (width, height) = image.dimensions();
    Some((i64::from(width), i64::from(height)))
}

fn split_dimensions(dimensions: Option<(i64, i64)>) -> (Option<i64>, Option<i64>) {
    dimensions
        .map(|(width, height)| (Some(width), Some(height)))
        .unwrap_or((None, None))
}

fn pdf_page_dimensions(document: &PdfDocument, page_number: u32) -> Option<(i64, i64)> {
    let object_id = *document.get_pages().get(&page_number)?;
    let page = document.get_dictionary(object_id).ok()?;
    let media_box = page.get(b"MediaBox").ok()?.as_array().ok()?;
    if media_box.len() != 4 {
        return None;
    }

    let left = pdf_numeric_value(&media_box[0])?;
    let bottom = pdf_numeric_value(&media_box[1])?;
    let right = pdf_numeric_value(&media_box[2])?;
    let top = pdf_numeric_value(&media_box[3])?;
    let width = (right - left).abs().round();
    let height = (top - bottom).abs().round();
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some((width as i64, height as i64))
}

fn pdf_numeric_value(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

pub(in crate::task_queue) fn is_supported_page_image_file_name(file_name: &str) -> bool {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp"
            )
        })
}

pub(in crate::task_queue) fn is_epub_page_resource_file_name(file_name: &str) -> bool {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "xhtml" | "html" | "htm"
            )
        })
}

pub(in crate::task_queue) fn media_type_from_entry_name(file_name: &str) -> String {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .map(|extension| match extension.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "png" => "image/png",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "avif" => "image/avif",
            "bmp" => "image/bmp",
            "xhtml" | "html" | "htm" => "application/xhtml+xml",
            "pdf" => "application/pdf",
            _ => "application/octet-stream",
        })
        .unwrap_or("application/octet-stream")
        .to_string()
}

pub(in crate::task_queue) fn expected_extension_for_media_type(
    media_type: &str,
) -> Option<&'static str> {
    match media_type {
        "application/vnd.comicbook-rar"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => Some("cbr"),
        "application/zip" => Some("cbz"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        _ => None,
    }
}

pub(in crate::task_queue) fn is_rar_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/x-rar-compressed; version=4" | "application/x-rar-compressed; version=5"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn analyze_book_media_file_marks_invalid_pdf_as_error_instead_of_ready() {
        let fixture_path = std::env::temp_dir().join(format!(
            "komga-invalid-pdf-{}.pdf",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        fs::write(&fixture_path, b"not a real pdf").expect("invalid pdf fixture should be written");

        let analysis = analyze_book_media_file(&fixture_path, "broken.pdf", false)
            .expect("invalid pdf analysis should not raise runtime error");

        assert_eq!(analysis.status, "ERROR");
        assert!(analysis.pages.is_empty());

        let _ = fs::remove_file(fixture_path);
    }

    #[test]
    fn analyze_book_media_file_detects_rar4_versioned_media_type() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = analyze_book_media_file(&fixture_path, "archives/rar4.cbr", false)
            .expect("rar4 fixture analysis should succeed");

        assert_eq!(analysis.status, "READY");
        assert_eq!(
            analysis.media_type,
            "application/x-rar-compressed; version=4"
        );
        assert!(!analysis.pages.is_empty());
    }

    #[test]
    fn is_rar_media_type_accepts_kotlin_versioned_rar_media_types() {
        assert!(is_rar_media_type("application/x-rar-compressed; version=4"));
        assert!(is_rar_media_type("application/x-rar-compressed; version=5"));
        assert!(!is_rar_media_type("application/vnd.comicbook-rar"));
        assert!(!is_rar_media_type("application/x-rar-compressed"));
    }
}
