use flate2::read::GzDecoder;
use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, QueryRestrictions,
    content_allowed_by_restrictions,
};
use serde_json::{Value, json};
use std::io::Read;

use crate::discovery::{BookDetailPort, BookMetadataAuthorReadModel, SeriesDetailPort};
use crate::identity_access::{AuthUser, user_shared_all_libraries, user_shared_library_ids};
use crate::media_assets::{
    BookMediaRecord, BookPageRecord, ContentResolverPort, MediaReaderPort, PersistedMediaFileRecord,
};

const EPUB_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/epub";
const PDF_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/pdf";
const DIVINA_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/divina";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManifestProfile {
    Epub,
    Pdf,
    Divina,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestVariant {
    Default,
    Epub,
    Pdf,
    Divina,
}

pub struct PersistedManifest {
    pub content_type: &'static str,
    pub payload: Value,
    pub series_id: Option<String>,
}

pub enum ManifestBuildOutcome {
    Found(PersistedManifest),
    BadRequest(String),
    NotFound,
    Forbidden,
}

struct ManifestUserContext {
    allowed_library_ids: Option<Vec<String>>,
    age: Option<u16>,
    age_restriction: Option<ManifestAgeRestrictionKind>,
    labels_allow: Vec<String>,
    labels_exclude: Vec<String>,
}

#[derive(Clone, Copy)]
enum ManifestAgeRestrictionKind {
    AllowOnly,
    Exclude,
}

struct WebpubMetadataAdditions {
    metadata: serde_json::Map<String, Value>,
    epub_divina_compatible: bool,
    series_id: String,
}

impl ManifestUserContext {
    fn from_auth_user(user: &AuthUser) -> Self {
        let allowed_library_ids = if user_shared_all_libraries(user) {
            None
        } else {
            Some(user_shared_library_ids(user).to_vec())
        };
        let age = user
            .age_restriction
            .as_ref()
            .and_then(|restriction| u16::try_from(restriction.age).ok());
        let age_restriction =
            user.age_restriction.as_ref().and_then(|restriction| {
                match restriction.restriction.trim().to_ascii_uppercase().as_str() {
                    "ALLOW_ONLY" => Some(ManifestAgeRestrictionKind::AllowOnly),
                    "EXCLUDE" => Some(ManifestAgeRestrictionKind::Exclude),
                    _ => None,
                }
            });

        Self {
            allowed_library_ids,
            age,
            age_restriction,
            labels_allow: normalized_labels(&user.labels_allow),
            labels_exclude: normalized_labels(&user.labels_exclude),
        }
    }

    fn can_access_library(&self, library_id: &str) -> bool {
        match &self.allowed_library_ids {
            None => true,
            Some(ids) => ids.iter().any(|id| id == library_id),
        }
    }

    fn content_allowed(&self, age_rating: Option<u16>, sharing_labels: &[String]) -> bool {
        let restrictions = QueryRestrictions {
            age: self.age,
            age_restriction: self.age_restriction.map(|kind| match kind {
                ManifestAgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
                ManifestAgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
            }),
            labels_allow: self.labels_allow.clone(),
            labels_exclude: self.labels_exclude.clone(),
        };
        content_allowed_by_restrictions(&restrictions, age_rating, sharing_labels)
    }
}

fn normalized_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn manifest_profile_from_media_type(media_type: &str) -> ManifestProfile {
    if media_type == "application/epub+zip" {
        ManifestProfile::Epub
    } else if media_type == "application/pdf" {
        ManifestProfile::Pdf
    } else {
        ManifestProfile::Divina
    }
}

fn manifest_content_type(variant: ManifestVariant, profile: ManifestProfile) -> &'static str {
    match variant {
        ManifestVariant::Divina => "application/divina+json",
        ManifestVariant::Default => match profile {
            ManifestProfile::Divina => "application/divina+json",
            ManifestProfile::Epub | ManifestProfile::Pdf => "application/webpub+json",
        },
        ManifestVariant::Epub | ManifestVariant::Pdf => "application/webpub+json",
    }
}

fn manifest_variant_matches_profile(
    variant: ManifestVariant,
    profile: ManifestProfile,
    epub_divina_compatible: bool,
) -> bool {
    match variant {
        ManifestVariant::Default => true,
        ManifestVariant::Epub => profile == ManifestProfile::Epub,
        ManifestVariant::Pdf => profile == ManifestProfile::Pdf,
        ManifestVariant::Divina => match profile {
            ManifestProfile::Divina | ManifestProfile::Pdf => true,
            ManifestProfile::Epub => epub_divina_compatible,
        },
    }
}

fn effective_manifest_profile(
    requested_variant: ManifestVariant,
    detected_profile: ManifestProfile,
) -> ManifestProfile {
    match requested_variant {
        ManifestVariant::Default => detected_profile,
        ManifestVariant::Epub => ManifestProfile::Epub,
        ManifestVariant::Pdf => ManifestProfile::Pdf,
        ManifestVariant::Divina => ManifestProfile::Divina,
    }
}

fn effective_manifest_variant(
    requested_variant: ManifestVariant,
    profile: ManifestProfile,
) -> ManifestVariant {
    match requested_variant {
        ManifestVariant::Default => match profile {
            ManifestProfile::Epub => ManifestVariant::Epub,
            ManifestProfile::Pdf => ManifestVariant::Pdf,
            ManifestProfile::Divina => ManifestVariant::Divina,
        },
        other => other,
    }
}

fn persisted_manifest_payload(
    book_id: &str,
    title: &str,
    media_type: &str,
    manifest_type: &str,
    reading_order: Vec<Value>,
) -> Value {
    json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": {
            "title": title,
        },
        "links": [
            {
                "rel": "self",
                "href": api_manifest_path(book_id),
                "type": manifest_type,
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": api_file_path(book_id),
                "type": media_type,
            }
        ],
        "readingOrder": reading_order,
        "resources": [
            {
                "href": api_thumbnail_path(book_id),
                "type": "image/jpeg",
            }
        ],
        "toc": [],
        "landmarks": [],
        "pageList": [],
    })
}

fn api_manifest_path(book_id: &str) -> String {
    format!("/api/v1/books/{book_id}/manifest")
}

fn api_file_path(book_id: &str) -> String {
    format!("/api/v1/books/{book_id}/file")
}

fn api_thumbnail_path(book_id: &str) -> String {
    format!("/api/v1/books/{book_id}/thumbnail")
}

fn api_divina_manifest_path(book_id: &str) -> String {
    format!("/api/v1/books/{book_id}/manifest/divina")
}

fn api_resource_path(book_id: &str, file_name: &str) -> String {
    format!(
        "/api/v1/books/{book_id}/resource/{}",
        file_name.trim_start_matches('/')
    )
}

fn api_raw_page_path(book_id: &str, page: u64) -> String {
    format!("/api/v1/books/{book_id}/pages/{page}/raw")
}

fn api_page_path(book_id: &str, page: u64) -> String {
    format!("/api/v1/books/{book_id}/pages/{page}?contentNegotiation=false")
}

fn api_page_jpeg_path(book_id: &str, page: u64) -> String {
    format!("/api/v1/books/{book_id}/pages/{page}?contentNegotiation=false&convert=jpeg")
}

fn profile_conforms_to(profile: ManifestProfile) -> &'static str {
    match profile {
        ManifestProfile::Epub => EPUB_PROFILE_URL,
        ManifestProfile::Pdf => PDF_PROFILE_URL,
        ManifestProfile::Divina => DIVINA_PROFILE_URL,
    }
}

fn manifest_extra_links(
    book_id: &str,
    profile: ManifestProfile,
    epub_divina_compatible: bool,
) -> Vec<Value> {
    match profile {
        ManifestProfile::Pdf => vec![json!({
            "href": api_divina_manifest_path(book_id),
            "type": "application/divina+json",
        })],
        ManifestProfile::Epub if epub_divina_compatible => vec![json!({
            "href": api_divina_manifest_path(book_id),
            "type": "application/divina+json",
        })],
        ManifestProfile::Epub | ManifestProfile::Divina => Vec::new(),
    }
}

fn enrich_manifest_metadata(
    payload: &mut Value,
    metadata_additions: Option<&serde_json::Map<String, Value>>,
    profile: ManifestProfile,
) {
    if let Some(metadata) = payload.get_mut("metadata").and_then(Value::as_object_mut) {
        if let Some(metadata_additions) = metadata_additions {
            for (key, value) in metadata_additions {
                metadata.insert(key.clone(), value.clone());
            }
        }
        metadata.insert(
            "conformsTo".to_string(),
            Value::String(profile_conforms_to(profile).to_string()),
        );
    }
}

fn append_manifest_extra_links(
    payload: &mut Value,
    book_id: &str,
    profile: ManifestProfile,
    epub_divina_compatible: bool,
) {
    let extra_links = manifest_extra_links(book_id, profile, epub_divina_compatible);
    if extra_links.is_empty() {
        return;
    }
    if let Some(links) = payload.get_mut("links").and_then(Value::as_array_mut) {
        links.extend(extra_links);
    }
}

fn epub_resource_href(book_id: &str, file_name: &str) -> String {
    api_resource_path(book_id, file_name)
}

fn decode_epub_extension_payload(blob: &[u8]) -> Result<Value, String> {
    let mut decoder = GzDecoder::new(blob);
    let mut json = String::new();
    decoder
        .read_to_string(&mut json)
        .map_err(|error| format!("decode epub extension blob: {error}"))?;
    serde_json::from_str::<Value>(&json)
        .map_err(|error| format!("parse epub extension blob json: {error}"))
}

fn epub_link(entry: &PersistedMediaFileRecord, book_id: &str) -> Value {
    json!({
        "href": epub_resource_href(book_id, entry.file_name.as_str()),
        "type": entry.media_type,
    })
}

fn epub_sub_type_matches(entry: &PersistedMediaFileRecord, expected: &str) -> bool {
    entry
        .sub_type
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn epub_nav_href(book_id: &str, href: &str) -> String {
    let (path, fragment) = href
        .split_once('#')
        .map_or((href, None), |(path, fragment)| (path, Some(fragment)));
    let mut absolute = epub_resource_href(book_id, path);
    if let Some(fragment) = fragment
        && !fragment.is_empty()
    {
        absolute.push('#');
        absolute.push_str(fragment);
    }
    absolute
}

fn epub_nav_link(book_id: &str, entry: &Value) -> Value {
    let mut link = serde_json::Map::new();
    if let Some(title) = entry.get("title").cloned() {
        link.insert("title".to_string(), title);
    }
    if let Some(href) = entry.get("href").and_then(Value::as_str) {
        link.insert(
            "href".to_string(),
            Value::String(epub_nav_href(book_id, href)),
        );
    }
    let children = entry
        .get("children")
        .and_then(Value::as_array)
        .map(|children| {
            children
                .iter()
                .map(|child| epub_nav_link(book_id, child))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !children.is_empty() {
        link.insert("children".to_string(), Value::Array(children));
    }
    Value::Object(link)
}

fn epub_nav_links(
    extension_payload: Option<&Value>,
    field_name: &str,
    book_id: &str,
) -> Vec<Value> {
    extension_payload
        .and_then(|payload| payload.get(field_name))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| epub_nav_link(book_id, entry))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn persisted_epub_manifest_payload(
    book_id: &str,
    title: &str,
    media_type: &str,
    media_files: &[PersistedMediaFileRecord],
    extension_blob: Option<&[u8]>,
    metadata_additions: Option<&serde_json::Map<String, Value>>,
    epub_divina_compatible: bool,
) -> Result<Value, String> {
    let reading_order = media_files
        .iter()
        .filter(|entry| epub_sub_type_matches(entry, "EPUB_PAGE"))
        .map(|entry| epub_link(entry, book_id))
        .collect::<Vec<_>>();
    let extension_payload = extension_blob
        .map(decode_epub_extension_payload)
        .transpose()?;
    let resources = media_files
        .iter()
        .filter(|entry| epub_sub_type_matches(entry, "EPUB_ASSET"))
        .map(|entry| epub_link(entry, book_id))
        .collect::<Vec<_>>();

    let mut payload = persisted_manifest_payload(
        book_id,
        title,
        media_type,
        manifest_content_type(ManifestVariant::Epub, ManifestProfile::Epub),
        if reading_order.is_empty() {
            vec![default_reading_order_entry(book_id, media_type)]
        } else {
            reading_order
        },
    );

    enrich_manifest_metadata(&mut payload, metadata_additions, ManifestProfile::Epub);
    if let Some(metadata) = payload.get_mut("metadata").and_then(Value::as_object_mut)
        && let Some(is_fixed_layout) = extension_payload
            .as_ref()
            .and_then(|payload| payload.get("isFixedLayout"))
            .and_then(Value::as_bool)
    {
        metadata.insert(
            "rendition".to_string(),
            json!({
                "layout": if is_fixed_layout { "fixed" } else { "reflowable" },
            }),
        );
    }

    if let Some(resource_entries) = payload.get_mut("resources").and_then(Value::as_array_mut) {
        resource_entries.extend(resources);
    }

    if let Some(payload_map) = payload.as_object_mut() {
        payload_map.insert(
            "toc".to_string(),
            Value::Array(epub_nav_links(extension_payload.as_ref(), "toc", book_id)),
        );
        payload_map.insert(
            "landmarks".to_string(),
            Value::Array(epub_nav_links(
                extension_payload.as_ref(),
                "landmarks",
                book_id,
            )),
        );
        payload_map.insert(
            "pageList".to_string(),
            Value::Array(epub_nav_links(
                extension_payload.as_ref(),
                "pageList",
                book_id,
            )),
        );
    }

    append_manifest_extra_links(
        &mut payload,
        book_id,
        ManifestProfile::Epub,
        epub_divina_compatible,
    );

    Ok(payload)
}

#[allow(clippy::too_many_arguments)]
async fn build_manifest_reading_order(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    book_id: &str,
    media: &BookMediaRecord,
    media_type: &str,
    variant: ManifestVariant,
    profile: ManifestProfile,
) -> Result<Vec<Value>, String> {
    Ok(match (variant, profile) {
        (ManifestVariant::Pdf, ManifestProfile::Pdf) => (1..=media.page_count.max(1))
            .map(|page| {
                json!({
                    "href": api_raw_page_path(book_id, page),
                    "type": "application/pdf",
                })
            })
            .collect::<Vec<_>>(),
        (ManifestVariant::Divina, ManifestProfile::Pdf)
        | (ManifestVariant::Divina, ManifestProfile::Divina) => {
            let persisted_rows = reader.book_pages(book_id).await?;
            let effective_rows = if !persisted_rows.is_empty() {
                reading_order_entries(
                    book_id,
                    persisted_rows,
                    (profile == ManifestProfile::Pdf).then_some("image/jpeg"),
                )
            } else if let Some(archive_rows) = content.archive_page_rows(media).await {
                reading_order_entries(book_id, archive_rows, None)
            } else {
                reading_order_entries(book_id, content.generated_pdf_page_rows(media), None)
            };

            if effective_rows.is_empty() {
                vec![default_reading_order_entry(book_id, media_type)]
            } else {
                effective_rows
            }
        }
        _ => vec![default_reading_order_entry(book_id, media_type)],
    })
}

fn reading_order_entries(
    book_id: &str,
    page_rows: Vec<BookPageRecord>,
    media_type_override: Option<&str>,
) -> Vec<Value> {
    const RECOMMENDED_IMAGE_MEDIA_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif"];

    page_rows
        .into_iter()
        .map(|page| {
            let effective_media_type = media_type_override.unwrap_or(page.media_type.as_str());
            let mut entry = serde_json::Map::from_iter([
                (
                    "href".to_string(),
                    Value::String(api_page_path(book_id, page.number)),
                ),
                (
                    "type".to_string(),
                    Value::String(effective_media_type.to_string()),
                ),
                (
                    "width".to_string(),
                    page.width.map_or(Value::Null, Value::from),
                ),
                (
                    "height".to_string(),
                    page.height.map_or(Value::Null, Value::from),
                ),
            ]);

            if effective_media_type.starts_with("image/")
                && !RECOMMENDED_IMAGE_MEDIA_TYPES.contains(&effective_media_type)
            {
                entry.insert(
                    "alternate".to_string(),
                    Value::Array(vec![json!({
                        "href": api_page_jpeg_path(book_id, page.number),
                        "type": "image/jpeg",
                        "width": page.width,
                        "height": page.height,
                    })]),
                );
            }

            Value::Object(entry)
        })
        .collect()
}

fn default_reading_order_entry(book_id: &str, media_type: &str) -> Value {
    json!({
        "href": api_page_path(book_id, 1),
        "type": media_type,
    })
}

async fn load_persisted_webpub_metadata_additions(
    book_detail: &dyn BookDetailPort,
    series_detail: &dyn SeriesDetailPort,
    book_id: &str,
) -> Result<Option<WebpubMetadataAdditions>, String> {
    let Some(book) = book_detail
        .load_persisted_book_detail(book_id, None)
        .await?
    else {
        return Ok(None);
    };
    let series_id = book.series_id.clone();
    let series = series_detail
        .load_persisted_series_detail(&book.series_id)
        .await?;

    let mut metadata = serde_json::Map::new();
    if !book.metadata_summary.is_empty() {
        metadata.insert(
            "description".to_string(),
            Value::String(book.metadata_summary.clone()),
        );
    }
    if !book.metadata_isbn.is_empty() {
        metadata.insert(
            "identifier".to_string(),
            Value::String(format!("urn:isbn:{}", book.metadata_isbn)),
        );
    }
    if book.media_pages_count > 0 {
        metadata.insert(
            "numberOfPages".to_string(),
            Value::Number(book.media_pages_count.into()),
        );
    }
    if let Some(release_date) = book
        .metadata_release_date
        .as_ref()
        .filter(|value| !value.is_empty())
    {
        metadata.insert("published".to_string(), Value::String(release_date.clone()));
    }
    if !book.last_modified.is_empty() {
        metadata.insert(
            "modified".to_string(),
            Value::String(normalize_webpub_modified(&book.last_modified)),
        );
    }
    if !book.metadata_tags.is_empty() {
        metadata.insert(
            "subject".to_string(),
            Value::Array(
                book.metadata_tags
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    extend_webpub_metadata_with_role_authors(&mut metadata, &book.metadata_authors);
    if !book.series_title.is_empty() {
        let mut series_entry = serde_json::Map::new();
        series_entry.insert("name".to_string(), Value::String(book.series_title.clone()));
        if let Some(position) = serde_json::Number::from_f64(book.metadata_number_sort) {
            series_entry.insert("position".to_string(), Value::Number(position));
        }
        metadata.insert(
            "belongsTo".to_string(),
            Value::Object(serde_json::Map::from_iter([(
                "series".to_string(),
                Value::Array(vec![Value::Object(series_entry)]),
            )])),
        );
    }

    if let Some(series) = series {
        if !series.language.is_empty() {
            metadata.insert("language".to_string(), Value::String(series.language));
        }
        if let Some(reading_progression) =
            webpub_reading_progression(series.reading_direction.as_str())
        {
            metadata.insert(
                "readingProgression".to_string(),
                Value::String(reading_progression.to_string()),
            );
        }
    }

    Ok(Some(WebpubMetadataAdditions {
        metadata,
        epub_divina_compatible: book.media_epub_divina_compatible,
        series_id,
    }))
}

fn webpub_reading_progression(reading_direction: &str) -> Option<&'static str> {
    match reading_direction.trim().to_ascii_uppercase().as_str() {
        "LEFT_TO_RIGHT" => Some("ltr"),
        "RIGHT_TO_LEFT" => Some("rtl"),
        "VERTICAL" | "WEBTOON" => Some("ttb"),
        _ => None,
    }
}

fn normalize_webpub_modified(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }
    if time::OffsetDateTime::parse(trimmed, &time::format_description::well_known::Rfc3339).is_ok()
    {
        return trimmed.to_string();
    }
    if let Some((date, time)) = trimmed.split_once(' ') {
        return format!("{date}T{time}Z");
    }
    if trimmed.contains('T') {
        return format!("{trimmed}Z");
    }
    trimmed.to_string()
}

fn extend_webpub_metadata_with_role_authors(
    metadata: &mut serde_json::Map<String, Value>,
    authors: &[BookMetadataAuthorReadModel],
) {
    let mut author = Vec::new();
    let mut translator = Vec::new();
    let mut editor = Vec::new();
    let mut artist = Vec::new();
    let mut illustrator = Vec::new();
    let mut letterer = Vec::new();
    let mut penciler = Vec::new();
    let mut colorist = Vec::new();
    let mut inker = Vec::new();
    let mut contributor = Vec::new();

    for entry in authors {
        let target = match entry.role.trim().to_ascii_lowercase().as_str() {
            "author" => &mut author,
            "translator" => &mut translator,
            "editor" => &mut editor,
            "artist" => &mut artist,
            "illustrator" => &mut illustrator,
            "letterer" => &mut letterer,
            "penciler" | "penciller" => &mut penciler,
            "colorist" => &mut colorist,
            "inker" => &mut inker,
            _ => &mut contributor,
        };
        if !entry.name.is_empty() {
            target.push(Value::String(entry.name.clone()));
        }
    }

    for (key, values) in [
        ("author", author),
        ("translator", translator),
        ("editor", editor),
        ("artist", artist),
        ("illustrator", illustrator),
        ("letterer", letterer),
        ("penciler", penciler),
        ("colorist", colorist),
        ("inker", inker),
        ("contributor", contributor),
    ] {
        if !values.is_empty() {
            metadata.insert(key.to_string(), Value::Array(values));
        }
    }
}

pub async fn build_persisted_book_manifest(
    reader: &dyn MediaReaderPort,
    content: &dyn ContentResolverPort,
    book_detail: &dyn BookDetailPort,
    series_detail: &dyn SeriesDetailPort,
    user: &AuthUser,
    book_id: &str,
    variant: ManifestVariant,
) -> Result<ManifestBuildOutcome, String> {
    let Some((library_id, title, media_type)) = reader.manifest_book(book_id).await? else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    let user = ManifestUserContext::from_auth_user(user);
    if !user.can_access_library(&library_id) {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let Some(media) = reader.book_media(book_id).await? else {
        return Ok(ManifestBuildOutcome::NotFound);
    };
    if !user.can_access_library(&media.library_id) {
        return Ok(ManifestBuildOutcome::Forbidden);
    }
    if let Some((age_rating, labels)) = reader.book_restrictions(book_id).await?
        && !user.content_allowed(age_rating, &labels)
    {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let profile = manifest_profile_from_media_type(&media_type);
    let webpub_additions =
        load_persisted_webpub_metadata_additions(book_detail, series_detail, book_id).await?;
    let epub_divina_compatible = webpub_additions
        .as_ref()
        .is_some_and(|additions| additions.epub_divina_compatible);
    if !manifest_variant_matches_profile(variant, profile, epub_divina_compatible) {
        if matches!(
            variant,
            ManifestVariant::Epub | ManifestVariant::Pdf | ManifestVariant::Divina
        ) {
            return Ok(ManifestBuildOutcome::BadRequest(format!(
                "Book media type '{media_type}' not compatible with requested profile"
            )));
        }
        return Ok(ManifestBuildOutcome::NotFound);
    }

    let effective_variant = effective_manifest_variant(variant, profile);
    let effective_profile = effective_manifest_profile(variant, profile);

    if matches!(
        (effective_variant, profile),
        (ManifestVariant::Epub, ManifestProfile::Epub)
    ) {
        let media_files = reader.media_file_records(book_id).await?;
        let extension_blob = reader.epub_extension_blob(book_id).await?;
        let payload = persisted_epub_manifest_payload(
            book_id,
            &title,
            &media_type,
            media_files.as_slice(),
            extension_blob.as_ref().map(|(_, blob)| blob.as_slice()),
            webpub_additions
                .as_ref()
                .map(|additions| &additions.metadata),
            epub_divina_compatible,
        )?;
        return Ok(ManifestBuildOutcome::Found(PersistedManifest {
            content_type: manifest_content_type(effective_variant, effective_profile),
            payload,
            series_id: webpub_additions.map(|additions| additions.series_id),
        }));
    }

    let reading_order = build_manifest_reading_order(
        reader,
        content,
        book_id,
        &media,
        &media_type,
        effective_variant,
        effective_profile,
    )
    .await?;

    let mut payload = persisted_manifest_payload(
        book_id,
        &title,
        &media_type,
        manifest_content_type(effective_variant, effective_profile),
        reading_order,
    );
    if matches!(
        effective_variant,
        ManifestVariant::Pdf | ManifestVariant::Divina
    ) {
        enrich_manifest_metadata(
            &mut payload,
            webpub_additions
                .as_ref()
                .map(|additions| &additions.metadata),
            effective_profile,
        );
    }
    append_manifest_extra_links(
        &mut payload,
        book_id,
        effective_profile,
        epub_divina_compatible,
    );
    Ok(ManifestBuildOutcome::Found(PersistedManifest {
        content_type: manifest_content_type(effective_variant, effective_profile),
        payload,
        series_id: webpub_additions.map(|additions| additions.series_id),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_access::{AuthUser, AuthUserAgeRestriction};

    #[test]
    fn persisted_manifest_payload_uses_root_relative_api_hrefs() {
        let payload = persisted_manifest_payload(
            "book-1",
            "Book One",
            "application/pdf",
            "application/webpub+json",
            vec![default_reading_order_entry("book-1", "application/pdf")],
        );

        assert_eq!(
            payload["links"][0]["href"],
            json!("/api/v1/books/book-1/manifest")
        );
        assert_eq!(
            payload["links"][1]["href"],
            json!("/api/v1/books/book-1/file")
        );
        assert_eq!(
            payload["resources"][0]["href"],
            json!("/api/v1/books/book-1/thumbnail")
        );
        assert_eq!(
            payload["readingOrder"][0]["href"],
            json!("/api/v1/books/book-1/pages/1?contentNegotiation=false")
        );
    }

    #[test]
    fn manifest_user_context_enforces_library_and_content_restrictions() {
        let user = AuthUser {
            id: "user-1".to_string(),
            email: "user@example.org".to_string(),
            password: String::new(),
            roles: Vec::new(),
            shared_all_libraries: false,
            shared_library_ids: vec!["library-1".to_string()],
            labels_allow: Vec::new(),
            labels_exclude: vec!["blocked".to_string()],
            age_restriction: Some(AuthUserAgeRestriction {
                age: 16,
                restriction: "EXCLUDE".to_string(),
            }),
        };

        let context = ManifestUserContext::from_auth_user(&user);

        assert!(context.can_access_library("library-1"));
        assert!(!context.can_access_library("library-2"));
        assert!(context.content_allowed(Some(12), &[]));
        assert!(!context.content_allowed(Some(18), &[]));
        assert!(!context.content_allowed(None, &["Blocked".to_string()]));
    }
}
