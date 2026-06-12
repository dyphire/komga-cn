use axum::http::HeaderMap;
use komga_application::media_assets::{
    ManifestContributor, ManifestHref, ManifestLinkItem, ManifestNavigationItem, ManifestProfile,
    ManifestReadingProgression, ManifestVariant, PersistedManifest,
};
use serde_json::{Value, json};

use crate::request_urls::app_absolute_url;

const EPUB_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/epub";
const PDF_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/pdf";
const DIVINA_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/divina";

#[derive(Clone, Copy)]
pub(crate) enum ManifestHrefSurface {
    ApiV1,
    OpdsV2,
}

pub(crate) fn manifest_content_type(manifest: &PersistedManifest) -> &'static str {
    match manifest.variant {
        ManifestVariant::Divina => "application/divina+json",
        ManifestVariant::Default => match manifest.profile {
            ManifestProfile::Divina => "application/divina+json",
            ManifestProfile::Epub | ManifestProfile::Pdf => "application/webpub+json",
        },
        ManifestVariant::Epub | ManifestVariant::Pdf => "application/webpub+json",
    }
}

pub(crate) fn render_manifest_payload(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
) -> Value {
    let content_type = manifest_content_type(manifest);
    let mut links = vec![
        json!({
            "rel": "self",
            "href": render_href(headers, manifest, surface, &ManifestHref::Manifest),
            "type": content_type,
        }),
        json!({
            "rel": "http://opds-spec.org/acquisition",
            "href": render_href(headers, manifest, surface, &ManifestHref::File),
            "type": manifest.media_type,
        }),
    ];
    if should_expose_divina_link(manifest) {
        links.push(json!({
            "href": render_href(headers, manifest, surface, &ManifestHref::DivinaManifest),
            "type": "application/divina+json",
        }));
    }

    json!({
        "context": "https://readium.org/webpub-manifest/context.jsonld",
        "metadata": render_metadata(manifest),
        "links": links,
        "readingOrder": manifest.reading_order.iter().map(|item| render_link_item(headers, manifest, surface, item)).collect::<Vec<_>>(),
        "resources": manifest.resources.iter().map(|item| render_link_item(headers, manifest, surface, item)).collect::<Vec<_>>(),
        "toc": render_navigation_items(headers, manifest, surface, &manifest.toc),
        "landmarks": render_navigation_items(headers, manifest, surface, &manifest.landmarks),
        "pageList": render_navigation_items(headers, manifest, surface, &manifest.page_list),
    })
}

fn should_expose_divina_link(manifest: &PersistedManifest) -> bool {
    manifest.profile == ManifestProfile::Pdf
        || (manifest.profile == ManifestProfile::Epub && manifest.epub_divina_compatible)
}

fn render_metadata(manifest: &PersistedManifest) -> Value {
    let mut metadata = serde_json::Map::new();
    metadata.insert("title".to_string(), Value::String(manifest.title.clone()));
    metadata.insert(
        "conformsTo".to_string(),
        Value::String(profile_conforms_to(manifest.profile).to_string()),
    );

    let additions = &manifest.metadata;
    insert_string(
        &mut metadata,
        "description",
        additions.description.as_deref(),
    );
    if let Some(isbn) = &additions.isbn {
        insert_string(
            &mut metadata,
            "identifier",
            Some(&format!("urn:isbn:{isbn}")),
        );
    }
    if let Some(number_of_pages) = additions.number_of_pages {
        metadata.insert(
            "numberOfPages".to_string(),
            Value::Number(number_of_pages.into()),
        );
    }
    insert_string(&mut metadata, "published", additions.published.as_deref());
    if let Some(modified) = &additions.modified {
        insert_string(
            &mut metadata,
            "modified",
            Some(&normalize_webpub_modified(modified)),
        );
    }
    if !additions.subjects.is_empty() {
        metadata.insert(
            "subject".to_string(),
            Value::Array(
                additions
                    .subjects
                    .iter()
                    .cloned()
                    .map(Value::String)
                    .collect(),
            ),
        );
    }
    extend_metadata_with_role_authors(&mut metadata, &additions.contributors);
    if let Some(series) = &additions.series {
        let mut series_entry = serde_json::Map::new();
        series_entry.insert("name".to_string(), Value::String(series.name.clone()));
        if let Some(position) = series.position.and_then(serde_json::Number::from_f64) {
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
    insert_string(&mut metadata, "language", additions.language.as_deref());
    if let Some(reading_progression) = additions.reading_progression {
        metadata.insert(
            "readingProgression".to_string(),
            Value::String(render_reading_progression(reading_progression).to_string()),
        );
    }
    if let Some(fixed_layout) = additions.fixed_layout {
        metadata.insert(
            "rendition".to_string(),
            json!({
                "layout": if fixed_layout { "fixed" } else { "reflowable" },
            }),
        );
    }

    Value::Object(metadata)
}

fn insert_string(metadata: &mut serde_json::Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        metadata.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn profile_conforms_to(profile: ManifestProfile) -> &'static str {
    match profile {
        ManifestProfile::Epub => EPUB_PROFILE_URL,
        ManifestProfile::Pdf => PDF_PROFILE_URL,
        ManifestProfile::Divina => DIVINA_PROFILE_URL,
    }
}

fn render_reading_progression(reading_progression: ManifestReadingProgression) -> &'static str {
    match reading_progression {
        ManifestReadingProgression::LeftToRight => "ltr",
        ManifestReadingProgression::RightToLeft => "rtl",
        ManifestReadingProgression::TopToBottom => "ttb",
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

fn extend_metadata_with_role_authors(
    metadata: &mut serde_json::Map<String, Value>,
    contributors: &[ManifestContributor],
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

    for entry in contributors {
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
        target.push(Value::String(entry.name.clone()));
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

fn render_link_item(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    item: &ManifestLinkItem,
) -> Value {
    let mut entry = serde_json::Map::from_iter([
        (
            "href".to_string(),
            Value::String(render_href(headers, manifest, surface, &item.href)),
        ),
        ("type".to_string(), Value::String(item.media_type.clone())),
    ]);
    if item.include_dimensions {
        entry.insert(
            "width".to_string(),
            item.width.map_or(Value::Null, Value::from),
        );
        entry.insert(
            "height".to_string(),
            item.height.map_or(Value::Null, Value::from),
        );
    }
    if !item.alternate.is_empty() {
        entry.insert(
            "alternate".to_string(),
            Value::Array(
                item.alternate
                    .iter()
                    .map(|alternate| render_link_item(headers, manifest, surface, alternate))
                    .collect(),
            ),
        );
    }
    Value::Object(entry)
}

fn render_navigation_items(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    items: &[ManifestNavigationItem],
) -> Value {
    Value::Array(
        items
            .iter()
            .map(|item| render_navigation_item(headers, manifest, surface, item))
            .collect(),
    )
}

fn render_navigation_item(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    item: &ManifestNavigationItem,
) -> Value {
    let mut entry = serde_json::Map::new();
    if let Some(title) = &item.title {
        entry.insert("title".to_string(), Value::String(title.clone()));
    }
    if let Some(href) = &item.href {
        entry.insert(
            "href".to_string(),
            Value::String(render_href(headers, manifest, surface, href)),
        );
    }
    if !item.children.is_empty() {
        entry.insert(
            "children".to_string(),
            render_navigation_items(headers, manifest, surface, &item.children),
        );
    }
    Value::Object(entry)
}

fn render_href(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    href: &ManifestHref,
) -> String {
    app_absolute_url(
        headers,
        manifest_path(manifest.book_id.as_str(), surface, href).as_str(),
    )
}

fn manifest_path(book_id: &str, surface: ManifestHrefSurface, href: &ManifestHref) -> String {
    let prefix = match surface {
        ManifestHrefSurface::ApiV1 => "/api/v1",
        ManifestHrefSurface::OpdsV2 => "/opds/v2",
    };
    match href {
        ManifestHref::Manifest => format!("{prefix}/books/{book_id}/manifest"),
        ManifestHref::File => format!("{prefix}/books/{book_id}/file"),
        ManifestHref::Thumbnail => format!("{prefix}/books/{book_id}/thumbnail"),
        ManifestHref::DivinaManifest => format!("{prefix}/books/{book_id}/manifest/divina"),
        ManifestHref::Resource(resource) => {
            format!(
                "{prefix}/books/{book_id}/resource/{}",
                resource.trim_start_matches('/')
            )
        }
        ManifestHref::RawPage(page) => format!("{prefix}/books/{book_id}/pages/{page}/raw"),
        ManifestHref::Page(page) => {
            format!("{prefix}/books/{book_id}/pages/{page}?contentNegotiation=false")
        }
        ManifestHref::PageJpeg(page) => {
            format!("{prefix}/books/{book_id}/pages/{page}?contentNegotiation=false&convert=jpeg")
        }
    }
}
