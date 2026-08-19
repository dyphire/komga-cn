use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use komga_domain::discovery::MediaStatus;
use komga_epub::{MOBI_MEDIA_TYPE, analyze_epub_file, normalize_mobi};
use lopdf::{Document as PdfDocument, Object};

use crate::rar_support::{detect_rar_media_type, list_rar_entries, read_rar_entries_bytes};

const IMAGE_DIMENSIONS_INITIAL_READ_BYTES: usize = 512;
const IMAGE_DIMENSIONS_READ_CHUNK_BYTES: usize = 16 * 1024;
const IMAGE_DIMENSIONS_MAX_READ_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MediaAnalysisProfile {
    PersistedBook { include_dimensions: bool },
    Transient,
}

impl MediaAnalysisProfile {
    fn include_dimensions(self) -> bool {
        match self {
            Self::PersistedBook { include_dimensions } => include_dimensions,
            Self::Transient => true,
        }
    }

    fn include_epub_resources(self) -> bool {
        matches!(self, Self::PersistedBook { .. })
    }

    fn supports_single_image(self) -> bool {
        matches!(self, Self::Transient)
    }

    fn includes_page_file_name(self, file_name: &str) -> bool {
        match self {
            Self::PersistedBook { .. } => is_supported_page_image_file_name(file_name),
            Self::Transient => is_transient_page_image_file_name(file_name),
        }
    }

    fn pdf_page_media_type(self) -> &'static str {
        match self {
            Self::PersistedBook { .. } => "application/pdf",
            Self::Transient => "image/jpeg",
        }
    }

    fn pdf_page_file_name(self, index: usize) -> String {
        match self {
            Self::PersistedBook { .. } => format!("page-{index:04}.pdf"),
            Self::Transient => (index + 1).to_string(),
        }
    }

    fn scale_pdf_dimensions(self) -> bool {
        matches!(self, Self::Transient)
    }

    fn records_analysis_error(self) -> bool {
        matches!(self, Self::PersistedBook { .. })
    }

    fn media_type_from_path(self, path: &Path) -> String {
        let detected = match self {
            Self::PersistedBook { .. } => persisted_media_type_from_path(path),
            Self::Transient => transient_media_type_from_path(path),
        };
        detected.unwrap_or_else(|| "application/octet-stream".to_string())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnalyzedMediaPage {
    pub(crate) file_name: String,
    pub(crate) media_type: String,
    pub(crate) width: Option<i64>,
    pub(crate) height: Option<i64>,
    pub(crate) file_size: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MediaFileAnalysis {
    pub(crate) status: MediaStatus,
    pub(crate) media_type: String,
    pub(crate) page_count: u64,
    pub(crate) epub_divina_compatible: bool,
    pub(crate) epub_is_kepub: bool,
    pub(crate) pages: Vec<AnalyzedMediaPage>,
    pub(crate) files: Vec<String>,
    pub(crate) media_files: Vec<AnalyzedMediaFile>,
    pub(crate) epub_extension_blob: Option<Vec<u8>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnalyzedMediaFile {
    pub(crate) file_name: String,
    pub(crate) media_type: String,
    pub(crate) sub_type: String,
    pub(crate) file_size: i64,
}

pub(crate) struct MediaFileAnalyzer;

#[derive(Default)]
struct AnalyzedMediaFileContents {
    page_count: u64,
    epub_divina_compatible: bool,
    epub_is_kepub: bool,
    pages: Vec<AnalyzedMediaPage>,
    files: Vec<String>,
    media_files: Vec<AnalyzedMediaFile>,
    epub_extension_blob: Option<Vec<u8>>,
}

fn empty_media_analysis(status: MediaStatus, media_type: String) -> MediaFileAnalysis {
    MediaFileAnalysis {
        status,
        media_type,
        page_count: 0,
        epub_divina_compatible: false,
        epub_is_kepub: false,
        pages: Vec::new(),
        files: Vec::new(),
        media_files: Vec::new(),
        epub_extension_blob: None,
    }
}

impl MediaFileAnalyzer {
    pub(crate) fn analyze(
        &self,
        file_path: &Path,
        profile: MediaAnalysisProfile,
    ) -> anyhow::Result<MediaFileAnalysis> {
        let media_type = profile.media_type_from_path(file_path);

        match file_path.try_exists() {
            Ok(true) => {}
            Ok(false) => {
                return Ok(empty_media_analysis(MediaStatus::Error, media_type));
            }
            Err(error) => {
                return Err(anyhow::anyhow!(format!(
                    "check media file existence '{}': {error}",
                    file_path.display()
                )));
            }
        }

        let result = match media_type.as_str() {
            value if value.starts_with("image/") && profile.supports_single_image() => {
                analyze_single_image(file_path)
            }
            "application/zip" => analyze_zip_media_pages(file_path, false, profile),
            "application/epub+zip" if profile.include_epub_resources() => {
                analyze_epub_media_pages(file_path, profile)
            }
            "application/epub+zip" => analyze_zip_media_pages(file_path, false, profile),
            MOBI_MEDIA_TYPE => analyze_mobi_media_pages(file_path),
            "application/vnd.comicbook-rar"
            | "application/x-rar-compressed"
            | "application/x-rar-compressed; version=4"
            | "application/x-rar-compressed; version=5" => {
                analyze_rar_media_pages(file_path, profile)
            }
            "application/pdf" => analyze_pdf_media_pages(file_path, profile),
            _ => return Ok(empty_media_analysis(MediaStatus::Unsupported, media_type)),
        };

        let contents = match result {
            Ok(result) => result,
            Err(_) if profile.records_analysis_error() => {
                return Ok(empty_media_analysis(MediaStatus::Error, media_type));
            }
            Err(error) => return Err(error),
        };
        let page_count = contents.page_count.max(contents.pages.len() as u64);
        let status = if page_count == 0 {
            MediaStatus::Error
        } else {
            MediaStatus::Ready
        };
        Ok(MediaFileAnalysis {
            status,
            media_type,
            page_count,
            epub_divina_compatible: contents.epub_divina_compatible,
            epub_is_kepub: contents.epub_is_kepub,
            pages: contents.pages,
            files: contents.files,
            media_files: contents.media_files,
            epub_extension_blob: contents.epub_extension_blob,
        })
    }
}

pub(crate) fn transient_media_type_from_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let media_type = transient_media_type_from_file_name(file_name);
    match media_type {
        "application/vnd.comicbook-rar" => Some(detect_rar_media_type(path).to_string()),
        "application/epub+zip" => Some(detect_epub_media_type(path).to_string()),
        _ => Some(media_type.to_string()),
    }
}

fn persisted_media_type_from_path(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let media_type = persisted_media_type_from_file_name(file_name);
    match media_type {
        "application/vnd.comicbook-rar" => Some(detect_rar_media_type(path).to_string()),
        _ => Some(media_type.to_string()),
    }
}

pub(crate) fn media_type_from_entry_name(file_name: &str) -> String {
    match extension(file_name).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("bmp") => "image/bmp",
        Some("xhtml") | Some("html") | Some("htm") => "application/xhtml+xml",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
    .to_string()
}

pub(crate) fn is_supported_page_image_file_name(file_name: &str) -> bool {
    matches!(
        extension(file_name).as_deref(),
        Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "avif" | "bmp")
    )
}

fn is_epub_page_resource_file_name(file_name: &str) -> bool {
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

pub(crate) fn expected_extension_for_media_type(media_type: &str) -> Option<&'static str> {
    match media_type {
        "application/vnd.comicbook-rar"
        | "application/x-rar-compressed"
        | "application/x-rar-compressed; version=4"
        | "application/x-rar-compressed; version=5" => Some("cbr"),
        "application/zip" => Some("cbz"),
        "application/pdf" => Some("pdf"),
        "application/epub+zip" => Some("epub"),
        MOBI_MEDIA_TYPE => Some("mobi"),
        _ => None,
    }
}

pub(crate) fn is_rar_media_type(media_type: &str) -> bool {
    matches!(
        media_type,
        "application/x-rar-compressed; version=4" | "application/x-rar-compressed; version=5"
    )
}

fn transient_media_type_from_file_name(file_name: &str) -> &'static str {
    match extension(file_name).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("pdf") => "application/pdf",
        Some("epub") => "application/epub+zip",
        Some("mobi") => MOBI_MEDIA_TYPE,
        Some("cbz") | Some("zip") => "application/zip",
        Some("cbr") | Some("rar") => "application/vnd.comicbook-rar",
        _ => "application/octet-stream",
    }
}

fn persisted_media_type_from_file_name(file_name: &str) -> &'static str {
    match extension(file_name).as_deref() {
        Some("cbz") | Some("zip") => "application/zip",
        Some("cbr") | Some("rar") => "application/vnd.comicbook-rar",
        Some("pdf") => "application/pdf",
        Some("epub") => "application/epub+zip",
        Some("mobi") => MOBI_MEDIA_TYPE,
        _ => "application/octet-stream",
    }
}

fn is_transient_page_image_file_name(file_name: &str) -> bool {
    matches!(
        extension(file_name).as_deref(),
        Some("jpg" | "jpeg" | "png" | "gif" | "webp" | "avif")
    )
}

fn extension(file_name: &str) -> Option<String> {
    PathBuf::from(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct MediaDimensions {
    width: i64,
    height: i64,
}

fn image_dimensions_from_bytes_i64(bytes: &[u8]) -> Option<MediaDimensions> {
    let dimensions = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some(MediaDimensions {
        width: i64::from(dimensions.0),
        height: i64::from(dimensions.1),
    })
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ImageDimensions {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

pub(crate) fn image_dimensions_from_bytes_u32(bytes: &[u8]) -> Option<ImageDimensions> {
    let dimensions = image_dimensions_from_bytes_i64(bytes)?;
    Some(ImageDimensions {
        width: dimensions.width.try_into().ok()?,
        height: dimensions.height.try_into().ok()?,
    })
}

fn image_dimensions_from_reader(reader: &mut dyn Read) -> std::io::Result<Option<MediaDimensions>> {
    let mut bytes = Vec::with_capacity(IMAGE_DIMENSIONS_INITIAL_READ_BYTES);
    let mut next_read_size = IMAGE_DIMENSIONS_INITIAL_READ_BYTES;
    let mut buffer = [0; 4096];

    loop {
        let remaining = IMAGE_DIMENSIONS_MAX_READ_BYTES.saturating_sub(bytes.len());
        if remaining == 0 {
            return Ok(None);
        }

        let mut bytes_left = next_read_size.min(remaining);
        let bytes_before_read = bytes.len();
        while bytes_left > 0 {
            let read_size = buffer.len().min(bytes_left);
            let bytes_read = reader.read(&mut buffer[..read_size])?;
            if bytes_read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..bytes_read]);
            bytes_left -= bytes_read;
        }

        let bytes_read = bytes.len() - bytes_before_read;
        if bytes_read == 0 {
            return Ok(image_dimensions_from_bytes_i64(&bytes));
        }

        if let Some(dimensions) = image_dimensions_from_bytes_i64(&bytes) {
            return Ok(Some(dimensions));
        }

        next_read_size = IMAGE_DIMENSIONS_READ_CHUNK_BYTES;
    }
}

fn pdf_page_dimensions(document: &PdfDocument, page_number: u32) -> Option<MediaDimensions> {
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

    Some(MediaDimensions {
        width: width as i64,
        height: height as i64,
    })
}

fn analyze_single_image(file_path: &Path) -> anyhow::Result<AnalyzedMediaFileContents> {
    let file_name = file_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();
    let metadata = std::fs::metadata(file_path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("read image metadata '{}': ", file_path.display()))
    })?;
    let size_bytes = i64::try_from(metadata.len()).map_err(|error| {
        anyhow::anyhow!(error).context(format!("image file too large '{}'", file_path.display()))
    })?;
    let bytes = std::fs::read(file_path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("read image bytes '{}': ", file_path.display()))
    })?;
    let dimensions = image_dimensions_from_bytes_i64(&bytes).ok_or_else(|| {
        anyhow::anyhow!(format!("decode image dimensions '{}'", file_path.display()))
    })?;
    let dimensions = analyzed_media_page_dimensions(Some(dimensions));

    Ok(AnalyzedMediaFileContents {
        pages: vec![AnalyzedMediaPage {
            file_name: file_name.clone(),
            media_type: media_type_from_entry_name(&file_name),
            width: dimensions.width,
            height: dimensions.height,
            file_size: size_bytes,
        }],
        files: vec![file_name],
        ..Default::default()
    })
}

fn analyze_zip_media_pages(
    file_path: &Path,
    include_epub_resources: bool,
    profile: MediaAnalysisProfile,
) -> anyhow::Result<AnalyzedMediaFileContents> {
    let file = std::fs::File::open(file_path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("open zip file '{}': ", file_path.display()))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        anyhow::anyhow!(error).context(format!("open zip archive '{}': ", file_path.display()))
    })?;

    let mut files = Vec::new();
    let mut pages = Vec::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| {
            anyhow::anyhow!(error).context(format!("read zip entry at index {index}"))
        })?;
        if entry.is_dir() {
            continue;
        }

        let file_name = entry
            .name()
            .map_err(|error| {
                anyhow::anyhow!(error).context(format!("read zip entry name at index {index}"))
            })?
            .trim()
            .to_string();
        if file_name.is_empty() {
            continue;
        }
        files.push(file_name.clone());

        let include = if include_epub_resources {
            is_epub_page_resource_file_name(&file_name)
        } else {
            profile.includes_page_file_name(&file_name)
        };
        if !include {
            continue;
        }

        let media_type = media_type_from_entry_name(&file_name);
        let dimensions = if profile.include_dimensions() && media_type.starts_with("image/") {
            Some(
                image_dimensions_from_reader(&mut entry)
                    .map_err(|error| {
                        anyhow::anyhow!(error)
                            .context(format!("read zip entry dimensions for '{file_name}'"))
                    })?
                    .ok_or_else(|| {
                        anyhow::anyhow!(format!("decode zip entry dimensions for '{file_name}'"))
                    })?,
            )
        } else {
            None
        };
        let dimensions = analyzed_media_page_dimensions(dimensions);
        pages.push(AnalyzedMediaPage {
            media_type,
            file_name,
            width: dimensions.width,
            height: dimensions.height,
            file_size: i64::try_from(entry.size()).unwrap_or(i64::MAX),
        });
    }

    files.sort();
    Ok(AnalyzedMediaFileContents {
        page_count: pages.len() as u64,
        pages,
        files,
        ..Default::default()
    })
}

fn analyze_epub_media_pages(
    file_path: &Path,
    profile: MediaAnalysisProfile,
) -> anyhow::Result<AnalyzedMediaFileContents> {
    let analysis = analyze_epub_file(file_path)
        .map_err(|error| anyhow::anyhow!(error).context("analyze EPUB publication"))?;
    let pages = analysis
        .pages
        .into_iter()
        .map(|page| {
            let dimensions = if profile.include_dimensions()
                && page.media_type.starts_with("image/")
            {
                Some(
                    read_epub_image_dimensions(file_path, &page.file_name)?.ok_or_else(|| {
                        anyhow::anyhow!(format!(
                            "decode EPUB image dimensions for '{}'",
                            page.file_name
                        ))
                    })?,
                )
            } else {
                None
            };
            let dimensions = analyzed_media_page_dimensions(dimensions);
            Ok(AnalyzedMediaPage {
                file_name: page.file_name,
                media_type: page.media_type,
                width: dimensions.width,
                height: dimensions.height,
                file_size: page.file_size,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let media_files = analysis
        .media_files
        .into_iter()
        .map(|file| AnalyzedMediaFile {
            file_name: file.file_name,
            media_type: file.media_type,
            sub_type: file.sub_type,
            file_size: file.file_size,
        })
        .collect();

    Ok(AnalyzedMediaFileContents {
        page_count: analysis.page_count,
        epub_divina_compatible: analysis.divina_compatible,
        epub_is_kepub: analysis.is_kepub,
        pages,
        files: analysis.files,
        media_files,
        epub_extension_blob: Some(analysis.extension_blob),
    })
}

fn read_epub_image_dimensions(
    file_path: &Path,
    file_name: &str,
) -> anyhow::Result<Option<MediaDimensions>> {
    let file = std::fs::File::open(file_path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("open EPUB image '{}': ", file_path.display()))
    })?;
    let mut archive = zip::ZipArchive::new(file).map_err(|error| {
        anyhow::anyhow!(error).context(format!(
            "open EPUB image archive '{}': ",
            file_path.display()
        ))
    })?;
    let mut entry = archive.by_name(file_name).map_err(|error| {
        anyhow::anyhow!(error).context(format!("read EPUB image '{file_name}'"))
    })?;
    image_dimensions_from_reader(&mut entry).map_err(|error| {
        anyhow::anyhow!(error).context(format!("read EPUB image dimensions '{file_name}'"))
    })
}

fn analyze_mobi_media_pages(file_path: &Path) -> anyhow::Result<AnalyzedMediaFileContents> {
    let bytes = std::fs::read(file_path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("read MOBI file '{}': ", file_path.display()))
    })?;
    let publication = normalize_mobi(&bytes).map_err(|error| {
        anyhow::anyhow!(error).context(format!("normalize MOBI file '{}': ", file_path.display()))
    })?;

    let mut files = publication
        .chapters
        .iter()
        .map(|chapter| chapter.path.clone())
        .chain(
            publication
                .resources
                .iter()
                .map(|resource| resource.path.clone()),
        )
        .collect::<Vec<_>>();
    files.push("OEBPS/content.opf".to_string());
    files.push("OEBPS/nav.xhtml".to_string());
    files.sort();

    let pages = publication
        .chapters
        .iter()
        .map(|chapter| AnalyzedMediaPage {
            file_name: chapter.path.clone(),
            media_type: "application/xhtml+xml".to_string(),
            width: None,
            height: None,
            file_size: 0,
        })
        .collect::<Vec<_>>();

    let mut media_files = publication
        .chapters
        .iter()
        .map(|chapter| AnalyzedMediaFile {
            file_name: chapter.path.clone(),
            media_type: "application/xhtml+xml".to_string(),
            sub_type: "EPUB_PAGE".to_string(),
            file_size: 0,
        })
        .collect::<Vec<_>>();
    media_files.extend(
        publication
            .resources
            .iter()
            .map(|resource| AnalyzedMediaFile {
                file_name: resource.path.clone(),
                media_type: resource.media_type.clone(),
                sub_type: "EPUB_ASSET".to_string(),
                file_size: resource.bytes.len().try_into().unwrap_or(i64::MAX),
            }),
    );
    media_files.extend([
        AnalyzedMediaFile {
            file_name: "OEBPS/content.opf".to_string(),
            media_type: "application/oebps-package+xml".to_string(),
            sub_type: "EPUB_ASSET".to_string(),
            file_size: 0,
        },
        AnalyzedMediaFile {
            file_name: "OEBPS/nav.xhtml".to_string(),
            media_type: "application/xhtml+xml".to_string(),
            sub_type: "EPUB_ASSET".to_string(),
            file_size: 0,
        },
    ]);

    Ok(AnalyzedMediaFileContents {
        page_count: publication.page_count,
        epub_divina_compatible: false,
        epub_is_kepub: false,
        pages,
        files,
        media_files,
        epub_extension_blob: Some(
            publication
                .epub_extension_blob()
                .map_err(|error| anyhow::anyhow!(error))?,
        ),
    })
}

fn analyze_rar_media_pages(
    file_path: &Path,
    profile: MediaAnalysisProfile,
) -> anyhow::Result<AnalyzedMediaFileContents> {
    let entries =
        list_rar_entries(file_path).map_err(|_| anyhow::anyhow!("read rar entries failed"))?;
    let mut files = entries
        .iter()
        .map(|entry| entry.file_name.clone())
        .collect::<Vec<_>>();
    files.sort();

    let pages = if profile.include_dimensions() {
        let mut pages = Vec::new();
        for entry in read_rar_entries_bytes(file_path)? {
            if !profile.includes_page_file_name(&entry.file_name) {
                continue;
            }
            let dimensions = image_dimensions_from_bytes_i64(&entry.bytes).ok_or_else(|| {
                anyhow::anyhow!(format!(
                    "decode rar entry dimensions for '{}'",
                    entry.file_name
                ))
            })?;
            pages.push(analyzed_rar_media_page(
                entry.file_name,
                entry.unpacked_size,
                Some(dimensions),
            ));
        }
        pages
    } else {
        entries
            .into_iter()
            .filter(|entry| profile.includes_page_file_name(&entry.file_name))
            .map(|entry| analyzed_rar_media_page(entry.file_name, entry.unpacked_size, None))
            .collect::<Vec<_>>()
    };

    Ok(AnalyzedMediaFileContents {
        pages,
        files,
        ..Default::default()
    })
}

fn analyze_pdf_media_pages(
    file_path: &Path,
    profile: MediaAnalysisProfile,
) -> anyhow::Result<AnalyzedMediaFileContents> {
    let document = PdfDocument::load(file_path).map_err(|error| {
        anyhow::anyhow!(error).context(format!("load pdf '{}': ", file_path.display()))
    })?;
    let page_count = document.get_pages().len();
    let pages = (0..page_count)
        .map(|index| {
            let dimensions = profile
                .include_dimensions()
                .then(|| pdf_page_dimensions(&document, (index + 1) as u32))
                .flatten()
                .map(|dimensions| {
                    if profile.scale_pdf_dimensions() {
                        scale_pdf_page_dimensions(dimensions)
                    } else {
                        dimensions
                    }
                });
            let dimensions = analyzed_media_page_dimensions(dimensions);

            AnalyzedMediaPage {
                file_name: profile.pdf_page_file_name(index),
                media_type: profile.pdf_page_media_type().to_string(),
                width: dimensions.width,
                height: dimensions.height,
                file_size: 0,
            }
        })
        .collect::<Vec<_>>();
    let files = file_path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| vec![value.to_string()])
        .unwrap_or_default();
    Ok(AnalyzedMediaFileContents {
        pages,
        files,
        ..Default::default()
    })
}

fn analyzed_rar_media_page(
    file_name: String,
    unpacked_size: u64,
    dimensions: Option<MediaDimensions>,
) -> AnalyzedMediaPage {
    let dimensions = analyzed_media_page_dimensions(dimensions);
    AnalyzedMediaPage {
        media_type: media_type_from_entry_name(&file_name),
        file_name,
        width: dimensions.width,
        height: dimensions.height,
        file_size: unpacked_size.try_into().unwrap_or(i64::MAX),
    }
}

struct AnalyzedMediaPageDimensions {
    width: Option<i64>,
    height: Option<i64>,
}

fn analyzed_media_page_dimensions(
    dimensions: Option<MediaDimensions>,
) -> AnalyzedMediaPageDimensions {
    dimensions
        .map(|dimensions| AnalyzedMediaPageDimensions {
            width: Some(dimensions.width),
            height: Some(dimensions.height),
        })
        .unwrap_or(AnalyzedMediaPageDimensions {
            width: None,
            height: None,
        })
}

fn pdf_numeric_value(object: &Object) -> Option<f64> {
    match object {
        Object::Integer(value) => Some(*value as f64),
        Object::Real(value) => Some((*value).into()),
        _ => None,
    }
}

fn scale_pdf_page_dimensions(dimensions: MediaDimensions) -> MediaDimensions {
    let min_edge = dimensions.width.min(dimensions.height) as f64;
    if min_edge <= 0.0 {
        return dimensions;
    }

    let scale = 3200.0 / min_edge;
    MediaDimensions {
        width: ((dimensions.width as f64) * scale).round().max(1.0) as i64,
        height: ((dimensions.height as f64) * scale).round().max(1.0) as i64,
    }
}

fn detect_epub_media_type(path: &Path) -> &'static str {
    let Ok(file) = std::fs::File::open(path) else {
        return "application/octet-stream";
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return "application/octet-stream";
    };

    if archive.by_name("META-INF/container.xml").is_ok() {
        "application/epub+zip"
    } else {
        "application/zip"
    }
}

#[cfg(test)]
mod tests {
    use std::fs::{self, File};
    use std::io::{self, Read, Write};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use image::{ImageBuffer, Rgba};
    use komga_domain::discovery::MediaStatus;
    use lopdf::{Document as PdfDocument, Object, Stream, dictionary};
    use zip::write::SimpleFileOptions;
    use zip::{CompressionMethod, ZipWriter};

    use super::{
        MOBI_MEDIA_TYPE, MediaAnalysisProfile, MediaDimensions, MediaFileAnalyzer,
        image_dimensions_from_bytes_i64, image_dimensions_from_reader, is_rar_media_type,
        persisted_media_type_from_path, transient_media_type_from_path,
    };

    struct CountingImageReader {
        bytes: Vec<u8>,
        position: usize,
    }

    impl CountingImageReader {
        fn new(bytes: Vec<u8>) -> Self {
            Self { bytes, position: 0 }
        }

        fn bytes_read(&self) -> usize {
            self.position
        }

        fn total_len(&self) -> usize {
            self.bytes.len()
        }
    }

    impl Read for CountingImageReader {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let available = self.bytes.len().saturating_sub(self.position);
            let bytes_to_read = available.min(buffer.len());
            if bytes_to_read == 0 {
                return Ok(0);
            }

            let end = self.position + bytes_to_read;
            buffer[..bytes_to_read].copy_from_slice(&self.bytes[self.position..end]);
            self.position = end;
            Ok(bytes_to_read)
        }
    }

    fn unique_temp_path(case: &str, extension: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        std::env::temp_dir().join(format!("komga-media-analysis-{case}-{nanos}.{extension}"))
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

    fn minimal_png_bytes() -> Vec<u8> {
        vec![
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48,
            0x44, 0x52, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x08, 0x02, 0x00, 0x00,
            0x00, 0xF8, 0x62, 0xEA, 0x0E, 0x00, 0x00, 0x00, 0x08, 0x49, 0x44, 0x41, 0x54, 0x78,
            0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01, 0x48, 0x06, 0x89, 0xD2, 0x00, 0x00, 0x00,
            0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
        ]
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
    fn media_file_analyzer_uses_one_boundary_for_transient_image_and_persisted_pdf() {
        let image_path = unique_temp_path("single-image", "png");
        fs::write(&image_path, png_bytes(3, 5)).expect("png fixture should be written");
        let pdf_path = unique_temp_path("single-page", "pdf");
        write_single_page_pdf(&pdf_path, 595, 842);

        let analyzer = MediaFileAnalyzer;
        let image_analysis = analyzer
            .analyze(&image_path, MediaAnalysisProfile::Transient)
            .expect("single image should analyze");
        let pdf_analysis = analyzer
            .analyze(
                &pdf_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: true,
                },
            )
            .expect("pdf should analyze");

        assert_eq!(image_analysis.status, MediaStatus::Ready);
        assert_eq!(image_analysis.media_type.as_str(), "image/png");
        assert_eq!(image_analysis.pages[0].width, Some(3));
        assert_eq!(image_analysis.pages[0].height, Some(5));
        assert_eq!(pdf_analysis.status, MediaStatus::Ready);
        assert_eq!(pdf_analysis.media_type.as_str(), "application/pdf");
        assert_eq!(pdf_analysis.pages[0].width, Some(595));
        assert_eq!(pdf_analysis.pages[0].height, Some(842));

        let _ = fs::remove_file(image_path);
        let _ = fs::remove_file(pdf_path);
    }

    #[test]
    fn image_dimensions_from_bytes_reads_header_without_decoding_full_image() {
        assert_eq!(
            image_dimensions_from_bytes_i64(&minimal_png_bytes()),
            Some(MediaDimensions {
                width: 32,
                height: 16,
            }),
        );
    }

    #[test]
    fn image_dimensions_from_reader_stops_after_dimensions_are_known() {
        let mut png_with_large_tail = minimal_png_bytes();
        png_with_large_tail.resize(1024 * 1024, 0xFF);
        let mut reader = CountingImageReader::new(png_with_large_tail);

        assert_eq!(
            image_dimensions_from_reader(&mut reader).expect("dimension read should succeed"),
            Some(MediaDimensions {
                width: 32,
                height: 16,
            }),
        );
        assert!(
            reader.bytes_read() < reader.total_len(),
            "dimensions should not require reading the whole image entry"
        );
    }

    #[test]
    fn persisted_analysis_marks_invalid_pdf_as_error_instead_of_runtime_failure() {
        let fixture_path = unique_temp_path("invalid-pdf", "pdf");
        fs::write(&fixture_path, b"not a real pdf").expect("invalid pdf fixture should be written");

        let analysis = MediaFileAnalyzer
            .analyze(
                &fixture_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect("persisted invalid pdf analysis should record media error");

        assert_eq!(analysis.status, MediaStatus::Error);
        assert_eq!(analysis.media_type, "application/pdf");
        assert!(analysis.pages.is_empty());

        let _ = fs::remove_file(fixture_path);
    }

    #[cfg(unix)]
    #[test]
    fn persisted_analysis_reports_filesystem_probe_errors_before_missing_file_status() {
        let parent_file = unique_temp_path("probe-parent-file", "tmp");
        fs::write(&parent_file, b"not a directory").expect("parent file fixture should be written");
        let media_path = parent_file.join("book.cbz");

        let error = MediaFileAnalyzer
            .analyze(
                &media_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect_err("filesystem probe error should fail analysis");

        assert!(
            error.to_string().contains("check media file existence"),
            "unexpected error: {error}"
        );

        let _ = fs::remove_file(parent_file);
    }

    #[test]
    fn persisted_epub_detection_stays_extension_based_while_transient_validates_container() {
        let path = unique_temp_path("zip-as-epub", "epub");
        write_zip_as_epub(&path);

        assert_eq!(
            persisted_media_type_from_path(path.as_path()).as_deref(),
            Some("application/epub+zip")
        );
        assert_eq!(
            transient_media_type_from_path(path.as_path()).as_deref(),
            Some("application/zip")
        );

        let _ = fs::remove_file(path);
    }

    #[test]
    fn persisted_analysis_detects_rar4_versioned_media_type() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = MediaFileAnalyzer
            .analyze(
                &fixture_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect("rar4 fixture analysis should succeed");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert_eq!(
            analysis.media_type,
            "application/x-rar-compressed; version=4"
        );
        assert!(!analysis.pages.is_empty());
    }

    #[test]
    fn persisted_mobi_analysis_keeps_mobi_media_type_when_payload_is_invalid() {
        let path = unique_temp_path("invalid-mobi", "mobi");
        let mut bytes = vec![0_u8; 68];
        bytes[60..68].copy_from_slice(b"BOOKMOBI");
        fs::write(&path, bytes).expect("invalid mobi fixture should be written");

        let analysis = MediaFileAnalyzer
            .analyze(
                &path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect("invalid mobi should be represented as media error");

        assert_eq!(analysis.media_type, MOBI_MEDIA_TYPE);
        assert_eq!(analysis.status, MediaStatus::Error);
        assert!(analysis.pages.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn persisted_mobi_analysis_reads_the_local_sample_when_available() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../sample/epub3.mobi");
        if !path.is_file() {
            return;
        }

        let analysis = MediaFileAnalyzer
            .analyze(
                &path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect("local MOBI sample should analyze");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert_eq!(analysis.media_type, MOBI_MEDIA_TYPE);
        assert!(!analysis.pages.is_empty());
        assert!(
            analysis
                .media_files
                .iter()
                .any(|file| file.sub_type == "EPUB_PAGE")
        );
        assert!(analysis.epub_extension_blob.is_some());
    }

    #[test]
    fn persisted_epub_analysis_keeps_reflowable_content_as_resources() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/epub/The Incomplete Theft - Ralph Burke.epub");

        let analysis = MediaFileAnalyzer
            .analyze(
                &fixture_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect("EPUB fixture analysis should succeed");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert!(!analysis.epub_divina_compatible);
        assert!(!analysis.epub_is_kepub);
        assert!(
            analysis.pages.is_empty(),
            "reflowable EPUB content must not be persisted as image pages"
        );
        assert_eq!(analysis.page_count, 14);
        assert!(
            analysis.epub_extension_blob.is_some(),
            "EPUB analysis must persist its extension metadata"
        );
    }

    #[test]
    fn persisted_epub_analysis_marks_complete_image_mapping_as_divina_compatible() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/epub3.epub");

        let analysis = MediaFileAnalyzer
            .analyze(
                &fixture_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: false,
                },
            )
            .expect("fixed-layout EPUB fixture analysis should succeed");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert!(analysis.epub_divina_compatible);
        assert_eq!(analysis.pages.len(), 2);
    }

    #[test]
    fn persisted_analysis_reads_rar_page_dimensions() {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        let analysis = MediaFileAnalyzer
            .analyze(
                &fixture_path,
                MediaAnalysisProfile::PersistedBook {
                    include_dimensions: true,
                },
            )
            .expect("rar4 fixture analysis should succeed");

        assert_eq!(analysis.status, MediaStatus::Ready);
        assert!(
            analysis
                .pages
                .iter()
                .any(|page| page.width == Some(48) && page.height == Some(48)),
            "rar analysis should populate page dimensions"
        );
    }

    #[test]
    fn persisted_and_transient_profiles_keep_distinct_archive_page_rules() {
        assert!(
            super::MediaAnalysisProfile::PersistedBook {
                include_dimensions: false,
            }
            .includes_page_file_name("page.bmp")
        );
        assert!(!super::MediaAnalysisProfile::Transient.includes_page_file_name("page.bmp"));
    }

    #[test]
    fn is_rar_media_type_accepts_kotlin_versioned_rar_media_types() {
        assert!(is_rar_media_type("application/x-rar-compressed; version=4"));
        assert!(is_rar_media_type("application/x-rar-compressed; version=5"));
        assert!(!is_rar_media_type("application/vnd.comicbook-rar"));
        assert!(!is_rar_media_type("application/x-rar-compressed"));
    }

    #[test]
    fn media_type_detection_is_shared_between_transient_and_persisted_paths() {
        let rar4 = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../komga/src/test/resources/archives/rar4.rar");

        assert_eq!(
            transient_media_type_from_path(rar4.as_path()).as_deref(),
            Some("application/x-rar-compressed; version=4")
        );
        assert_eq!(
            persisted_media_type_from_path(rar4.as_path()).as_deref(),
            Some("application/x-rar-compressed; version=4")
        );
    }
}
