use super::*;

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
) -> Result<BookMediaAnalysis, String> {
    let media_type = media_type_from_path(book_url);

    if !file_path.exists() {
        return Ok(BookMediaAnalysis {
            status: "ERROR".to_string(),
            media_type,
            pages: Vec::new(),
        });
    }

    let pages = match media_type.as_str() {
        "application/zip" => analyze_zip_media_pages(file_path, false)
            .unwrap_or_else(|_| single_file_media_analysis_pages(file_path, media_type.as_str())),
        "application/epub+zip" => analyze_zip_media_pages(file_path, true)
            .unwrap_or_else(|_| single_file_media_analysis_pages(file_path, media_type.as_str())),
        "application/vnd.comicbook-rar" | "application/x-rar-compressed" => {
            analyze_rar_media_pages(file_path).unwrap_or_else(|_| {
                single_file_media_analysis_pages(file_path, media_type.as_str())
            })
        }
        "application/pdf" => analyze_pdf_media_pages(file_path)
            .unwrap_or_else(|_| single_file_media_analysis_pages(file_path, media_type.as_str())),
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

pub(in crate::task_queue) fn single_file_media_analysis_pages(
    file_path: &PathBuf,
    media_type: &str,
) -> Vec<AnalyzedMediaPageRow> {
    let file_name = file_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let file_size = fs::metadata(file_path)
        .ok()
        .map(|metadata| metadata.len())
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default();

    vec![AnalyzedMediaPageRow {
        file_name,
        media_type: media_type.to_string(),
        file_size,
    }]
}

pub(in crate::task_queue) fn analyze_zip_media_pages(
    file_path: &PathBuf,
    include_epub_resources: bool,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let file = fs::File::open(file_path)
        .map_err(|error| format!("open zip file '{}': {error}", file_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("open zip archive '{}': {error}", file_path.display()))?;

    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let entry = archive
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

        let file_size = i64::try_from(entry.size()).unwrap_or(i64::MAX);
        pages.push(AnalyzedMediaPageRow {
            media_type: media_type_from_entry_name(&file_name),
            file_name,
            file_size,
        });
    }

    Ok(pages)
}

pub(in crate::task_queue) fn analyze_rar_media_pages(
    file_path: &PathBuf,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let output = Command::new("unrar")
        .arg("lb")
        .arg(file_path)
        .output()
        .map_err(|error| format!("run 'unrar lb' for '{}': {error}", file_path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "'unrar lb' failed for '{}': status {}",
            file_path.display(),
            output.status,
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && is_supported_page_image_file_name(line))
        .map(|file_name| AnalyzedMediaPageRow {
            file_name: file_name.to_string(),
            media_type: media_type_from_entry_name(file_name),
            file_size: 0,
        })
        .collect::<Vec<_>>())
}

pub(in crate::task_queue) fn analyze_pdf_media_pages(
    file_path: &PathBuf,
) -> Result<Vec<AnalyzedMediaPageRow>, String> {
    let document = lopdf::Document::load(file_path)
        .map_err(|error| format!("load pdf '{}': {error}", file_path.display()))?;
    let page_count = document.get_pages().len();
    Ok((0..page_count)
        .map(|index| AnalyzedMediaPageRow {
            file_name: format!("page-{index:04}.pdf"),
            media_type: "application/pdf".to_string(),
            file_size: 0,
        })
        .collect::<Vec<_>>())
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
        "application/vnd.comicbook-rar" | "application/x-rar-compressed" => Some("cbr"),
        "application/zip" => Some("cbz"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        _ => None,
    }
}

pub(in crate::task_queue) fn is_rar_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/vnd.comicbook-rar" | "application/x-rar-compressed"
    )
}
