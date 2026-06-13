use serde_json::{Value, json};

use super::{
    BookMediaPort, ContentResolverPort, EpubExtensionBlob, EpubNavigationExtension,
    EpubNavigationPosition,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EpubNavigationLoadError {
    MissingExtension,
    Internal(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EpubNavigationError {
    BadRequest(String),
    Internal(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct EpubNavigation {
    media_files: Vec<String>,
    extension: EpubNavigationExtension,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NormalizedEpubLocator {
    raw: Value,
    progression: f64,
    total_progression: Option<f64>,
}

impl NormalizedEpubLocator {
    pub fn raw(&self) -> &Value {
        &self.raw
    }

    pub fn into_raw(self) -> Value {
        self.raw
    }

    pub fn progression(&self) -> f64 {
        self.progression
    }

    pub fn total_progression(&self) -> Option<f64> {
        self.total_progression
    }
}

#[async_trait::async_trait]
pub trait EpubNavigationExtensionReaderPort: Send + Sync {
    async fn epub_extension_blob(&self, book_id: &str)
    -> Result<Option<EpubExtensionBlob>, String>;
}

#[async_trait::async_trait]
impl<T> EpubNavigationExtensionReaderPort for T
where
    T: BookMediaPort + ?Sized,
{
    async fn epub_extension_blob(
        &self,
        book_id: &str,
    ) -> Result<Option<EpubExtensionBlob>, String> {
        BookMediaPort::epub_extension_blob(self, book_id).await
    }
}

#[async_trait::async_trait]
pub trait EpubNavigationReaderPort: EpubNavigationExtensionReaderPort {
    async fn book_media_files(&self, book_id: &str) -> Result<Vec<String>, String>;
}

#[async_trait::async_trait]
impl<T> EpubNavigationReaderPort for T
where
    T: BookMediaPort + ?Sized,
{
    async fn book_media_files(&self, book_id: &str) -> Result<Vec<String>, String> {
        BookMediaPort::book_media_files(self, book_id).await
    }
}

pub trait EpubNavigationContentPort: Send + Sync {
    fn decode_epub_navigation_extension(
        &self,
        blob: &[u8],
    ) -> Result<EpubNavigationExtension, String>;
}

impl<T> EpubNavigationContentPort for T
where
    T: ContentResolverPort + ?Sized,
{
    fn decode_epub_navigation_extension(
        &self,
        blob: &[u8],
    ) -> Result<EpubNavigationExtension, String> {
        ContentResolverPort::decode_epub_navigation_extension(self, blob)
    }
}

pub async fn load_book_epub_navigation<R, C>(
    reader: &R,
    content: &C,
    book_id: &str,
) -> Result<EpubNavigation, EpubNavigationLoadError>
where
    R: EpubNavigationReaderPort + ?Sized,
    C: EpubNavigationContentPort + ?Sized,
{
    let media_files = reader
        .book_media_files(book_id)
        .await
        .map_err(EpubNavigationLoadError::Internal)?;
    let extension = load_book_epub_navigation_extension(reader, content, book_id).await?;

    Ok(EpubNavigation {
        media_files,
        extension,
    })
}

pub async fn load_book_epub_navigation_extension<R, C>(
    reader: &R,
    content: &C,
    book_id: &str,
) -> Result<EpubNavigationExtension, EpubNavigationLoadError>
where
    R: EpubNavigationExtensionReaderPort + ?Sized,
    C: EpubNavigationContentPort + ?Sized,
{
    let Some(extension_blob) = reader
        .epub_extension_blob(book_id)
        .await
        .map_err(EpubNavigationLoadError::Internal)?
    else {
        return Err(EpubNavigationLoadError::MissingExtension);
    };

    let extension = content
        .decode_epub_navigation_extension(&extension_blob.bytes)
        .map_err(EpubNavigationLoadError::Internal)?;

    Ok(extension)
}

pub async fn load_book_epub_positions<R, C>(
    reader: &R,
    content: &C,
    book_id: &str,
) -> Result<Option<Vec<Value>>, EpubNavigationLoadError>
where
    R: EpubNavigationExtensionReaderPort + ?Sized,
    C: EpubNavigationContentPort + ?Sized,
{
    let Some(extension_blob) = reader
        .epub_extension_blob(book_id)
        .await
        .map_err(EpubNavigationLoadError::Internal)?
    else {
        return Ok(None);
    };
    if !epub_extension_blob_is_epub(&extension_blob) {
        return Ok(None);
    }

    let positions = content
        .decode_epub_navigation_extension(&extension_blob.bytes)
        .map_err(EpubNavigationLoadError::Internal)?
        .positions
        .into_iter()
        .map(EpubNavigationPosition::into_raw)
        .collect::<Vec<_>>();
    if positions.is_empty() {
        Ok(None)
    } else {
        Ok(Some(positions))
    }
}

impl EpubNavigation {
    pub fn positions(&self) -> &[EpubNavigationPosition] {
        self.extension.positions.as_slice()
    }

    pub fn locator_for_page(&self, page: u64) -> Option<Value> {
        self.extension
            .positions
            .get(page.saturating_sub(1) as usize)
            .map(|position| position.raw().clone())
    }

    pub fn normalize_locator(
        &self,
        locator: &Value,
    ) -> Result<NormalizedEpubLocator, EpubNavigationError> {
        let href_base = normalized_href_base(
            locator
                .get("href")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        );
        if href_base.is_empty() {
            return Err(EpubNavigationError::BadRequest(
                "Resource does not exist in book: ".to_string(),
            ));
        }

        let Some(locator_progression) = locator_progression(locator) else {
            return Err(EpubNavigationError::BadRequest(
                "location.progression is required".to_string(),
            ));
        };

        if !self.resource_exists(href_base.as_str()) {
            return Err(EpubNavigationError::BadRequest(format!(
                "Resource does not exist in book: {href_base}"
            )));
        }

        let Some(matched_position) = self.matched_position(href_base.as_str(), locator_progression)
        else {
            return Err(EpubNavigationError::BadRequest(
                "Invalid progression".to_string(),
            ));
        };

        let normalized_locator = normalized_epub_locator(locator, &matched_position);
        Ok(NormalizedEpubLocator {
            total_progression: locator_total_progression(&normalized_locator),
            raw: normalized_locator,
            progression: locator_progression,
        })
    }

    pub fn koreader_locator_for_progress(
        &self,
        progress: &str,
    ) -> Result<Value, EpubNavigationError> {
        let Some(resource_index) = parse_koreader_epub_resource_index(progress) else {
            return Err(EpubNavigationError::BadRequest(format!(
                "Could not get Epub resource index from progress: {progress}"
            )));
        };
        let unique_hrefs = self.unique_hrefs();
        let Some(href) = unique_hrefs.get(resource_index) else {
            return Err(EpubNavigationError::Internal(format!(
                "Could not get Epub resource index from progress: {progress}"
            )));
        };
        let Some(matched_position) = self
            .extension
            .positions
            .iter()
            .find(|position| position.href() == Some(href.as_str()))
        else {
            return Err(EpubNavigationError::BadRequest(format!(
                "Could not get Epub resource index from progress: {progress}"
            )));
        };

        Ok(koreader_epub_locator(href, matched_position))
    }

    pub fn koreader_progress_for_locator(&self, locator: &Value) -> Option<String> {
        let href = locator.get("href").and_then(Value::as_str)?.trim();
        if href.is_empty() {
            return None;
        }

        self.unique_hrefs()
            .iter()
            .position(|value| value == href)
            .map(|index| format!("/body/DocFragment[{}].0", index + 1))
    }

    fn resource_exists(&self, href_base: &str) -> bool {
        if !self.media_files.is_empty() {
            return self
                .media_files
                .iter()
                .any(|file_name| normalized_href_base(file_name) == href_base);
        }

        self.extension
            .positions
            .iter()
            .any(|position| position_matches_href(position, href_base))
    }

    fn matched_position(
        &self,
        href_base: &str,
        locator_progression: f64,
    ) -> Option<EpubNavigationPosition> {
        let matching_positions = self
            .extension
            .positions
            .iter()
            .filter(|position| position_matches_href(position, href_base))
            .cloned()
            .collect::<Vec<_>>();

        matching_positions
            .iter()
            .find(|position| position.progression() == Some(locator_progression))
            .cloned()
            .or_else(|| {
                if self.extension.is_fixed_layout && matching_positions.len() == 1 {
                    return matching_positions.first().cloned();
                }

                let before = matching_positions
                    .iter()
                    .filter(|position| {
                        position
                            .progression()
                            .is_some_and(|value| value < locator_progression)
                    })
                    .max_by_key(|position| position.position())
                    .cloned();
                let after = matching_positions
                    .iter()
                    .filter(|position| {
                        position
                            .progression()
                            .is_some_and(|value| value > locator_progression)
                    })
                    .min_by_key(|position| position.position())
                    .cloned();

                match (before, after) {
                    (Some(before), Some(_)) => Some(before),
                    _ => None,
                }
            })
    }

    fn unique_hrefs(&self) -> Vec<String> {
        let mut unique_hrefs = Vec::<String>::new();
        for position in &self.extension.positions {
            let Some(position_href) = position.href() else {
                continue;
            };
            let position_href = position_href.trim();
            if position_href.is_empty() || unique_hrefs.iter().any(|value| value == position_href) {
                continue;
            }
            unique_hrefs.push(position_href.to_string());
        }
        unique_hrefs
    }
}

fn epub_extension_blob_is_epub(extension_blob: &EpubExtensionBlob) -> bool {
    extension_blob.extension_class.is_empty()
        || extension_blob
            .extension_class
            .to_ascii_lowercase()
            .contains("mediaextensionepub")
}

pub fn normalized_href_base(href: &str) -> String {
    let base = href.split('#').next().unwrap_or(href).trim_end_matches('#');
    percent_decode(base).trim_start_matches('/').to_string()
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hi = bytes[index + 1] as char;
            let lo = bytes[index + 2] as char;
            let parsed = hi
                .to_digit(16)
                .and_then(|hi| lo.to_digit(16).map(|lo| ((hi << 4) | lo) as u8));
            if let Some(byte) = parsed {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        if bytes[index] == b'+' {
            decoded.push(b' ');
        } else {
            decoded.push(bytes[index]);
        }
        index += 1;
    }
    String::from_utf8(decoded).unwrap_or_else(|_| value.to_string())
}

fn locator_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("progression"))
        .and_then(Value::as_f64)
}

fn locator_total_progression(locator: &Value) -> Option<f64> {
    locator
        .get("locations")
        .and_then(|value| value.get("totalProgression"))
        .and_then(Value::as_f64)
}

fn position_matches_href(position: &EpubNavigationPosition, href_base: &str) -> bool {
    position
        .href()
        .map(|value| normalized_href_base(value) == href_base)
        .unwrap_or(false)
}

fn normalized_epub_locator(locator: &Value, matched_position: &EpubNavigationPosition) -> Value {
    let mut locator = locator.clone();
    let Some(locator_map) = locator.as_object_mut() else {
        return locator;
    };

    locator_map.insert(
        "type".to_string(),
        matched_position
            .media_type()
            .cloned()
            .unwrap_or_else(|| Value::String(String::new())),
    );

    let current_kobo_span_missing = locator_map.get("koboSpan").is_none_or(Value::is_null);
    if current_kobo_span_missing && let Some(kobo_span) = matched_position.kobo_span().cloned() {
        locator_map.insert("koboSpan".to_string(), kobo_span);
    }

    if let Some(locations) = locator_map
        .get_mut("locations")
        .and_then(Value::as_object_mut)
        && let Some(total_progression) = matched_position.total_progression_value().cloned()
    {
        locations.insert("totalProgression".to_string(), total_progression);
    }

    locator
}

fn parse_koreader_epub_resource_index(progress: &str) -> Option<usize> {
    let normalized = progress.trim().to_ascii_lowercase();

    if let Some(index) =
        parse_koreader_doc_fragment_index(normalized.as_str(), "docfragment[", ']', true)
    {
        return Some(index);
    }

    parse_koreader_doc_fragment_index(normalized.as_str(), "#_doc_fragment_", '_', false)
}

fn parse_koreader_doc_fragment_index(
    progress: &str,
    prefix: &str,
    suffix: char,
    one_based: bool,
) -> Option<usize> {
    let start = progress.find(prefix)? + prefix.len();
    let tail = &progress[start..];
    let end = tail.find(suffix)?;
    let index = tail[..end].parse::<usize>().ok()?;
    if one_based {
        index.checked_sub(1)
    } else {
        Some(index)
    }
}

fn koreader_epub_locator(href: &str, matched_position: &EpubNavigationPosition) -> Value {
    let mut locator = json!({
        "href": href,
        "type": matched_position
            .media_type()
            .cloned()
            .unwrap_or_else(|| Value::String("application/xhtml+xml".to_string())),
        "locations": {
            "progression": 0.0,
            "totalProgression": matched_position
                .total_progression_value()
                .cloned()
                .unwrap_or(Value::Null),
        },
    });

    if let Some(kobo_span) = matched_position.kobo_span().cloned()
        && !kobo_span.is_null()
    {
        locator
            .as_object_mut()
            .expect("koreader epub locator should be an object")
            .insert("koboSpan".to_string(), kobo_span);
    }

    locator
}
