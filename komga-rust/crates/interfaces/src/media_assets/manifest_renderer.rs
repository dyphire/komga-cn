use anyhow::Result;
use axum::http::HeaderMap;
use komga_application::media_assets::{
    ManifestContributor, ManifestHref, ManifestLinkItem, ManifestNavigationItem, ManifestProfile,
    ManifestReadingProgression, ManifestVariant, PersistedManifest,
};

use crate::contracts::media_assets::{
    WebPubManifestBelongsToDto, WebPubManifestDto, WebPubManifestLinkDimensionsDto,
    WebPubManifestLinkDto, WebPubManifestMetadataDto, WebPubManifestNavigationDto,
    WebPubManifestRenditionDto, WebPubManifestSeriesDto,
};
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
) -> Result<WebPubManifestDto> {
    let content_type = manifest_content_type(manifest);
    let mut links = vec![
        WebPubManifestLinkDto {
            rel: Some("self".to_string()),
            href: render_href(headers, manifest, surface, &ManifestHref::Manifest),
            media_type: content_type.to_string(),
            properties: None,
            dimensions: None,
            alternate: vec![],
        },
        WebPubManifestLinkDto {
            rel: Some("http://opds-spec.org/acquisition".to_string()),
            href: render_href(headers, manifest, surface, &ManifestHref::File),
            media_type: manifest.media_type.clone(),
            properties: None,
            dimensions: None,
            alternate: vec![],
        },
    ];
    if should_expose_divina_link(manifest) {
        links.push(WebPubManifestLinkDto {
            rel: None,
            href: render_href(headers, manifest, surface, &ManifestHref::DivinaManifest),
            media_type: "application/divina+json".to_string(),
            properties: None,
            dimensions: None,
            alternate: vec![],
        });
    }

    Ok(WebPubManifestDto {
        context: "https://readium.org/webpub-manifest/context.jsonld".to_string(),
        metadata: render_metadata(manifest)?,
        links,
        reading_order: manifest
            .reading_order
            .iter()
            .map(|item| render_link_item(headers, manifest, surface, item))
            .collect(),
        resources: manifest
            .resources
            .iter()
            .map(|item| render_link_item(headers, manifest, surface, item))
            .collect(),
        toc: render_navigation_items(headers, manifest, surface, &manifest.toc),
        landmarks: render_navigation_items(headers, manifest, surface, &manifest.landmarks),
        page_list: render_navigation_items(headers, manifest, surface, &manifest.page_list),
    })
}

fn should_expose_divina_link(manifest: &PersistedManifest) -> bool {
    manifest.profile == ManifestProfile::Pdf
        || (manifest.profile == ManifestProfile::Epub && manifest.epub_divina_compatible)
}

fn render_metadata(manifest: &PersistedManifest) -> Result<WebPubManifestMetadataDto> {
    let additions = &manifest.metadata;
    let mut roles = RoleContributors::default();
    for contributor in &additions.contributors {
        roles.push(contributor);
    }

    Ok(WebPubManifestMetadataDto {
        title: manifest.title.clone(),
        conforms_to: profile_conforms_to(manifest.profile).to_string(),
        description: non_empty(additions.description.as_deref()),
        identifier: additions
            .isbn
            .as_deref()
            .map(|isbn| format!("urn:isbn:{isbn}")),
        number_of_pages: additions.number_of_pages,
        published: non_empty(additions.published.as_deref()),
        modified: additions
            .modified
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(crate::contracts::common::KotlinUtcDateTime::parse)
            .transpose()?,
        subject: (!additions.subjects.is_empty()).then(|| additions.subjects.clone()),
        author: roles.author,
        translator: roles.translator,
        editor: roles.editor,
        artist: roles.artist,
        illustrator: roles.illustrator,
        letterer: roles.letterer,
        penciler: roles.penciler,
        colorist: roles.colorist,
        inker: roles.inker,
        contributor: roles.contributor,
        belongs_to: additions
            .series
            .as_ref()
            .map(|series| WebPubManifestBelongsToDto {
                series: vec![WebPubManifestSeriesDto {
                    name: series.name.clone(),
                    position: series.position,
                    links: None,
                }],
            }),
        language: non_empty(additions.language.as_deref()),
        reading_progression: additions
            .reading_progression
            .map(render_reading_progression)
            .map(str::to_string),
        rendition: additions
            .fixed_layout
            .map(|fixed_layout| WebPubManifestRenditionDto {
                layout: if fixed_layout {
                    "fixed".to_string()
                } else {
                    "reflowable".to_string()
                },
            }),
    })
}

fn non_empty(value: Option<&str>) -> Option<String> {
    value
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

#[derive(Default)]
struct RoleContributors {
    author: Option<Vec<String>>,
    translator: Option<Vec<String>>,
    editor: Option<Vec<String>>,
    artist: Option<Vec<String>>,
    illustrator: Option<Vec<String>>,
    letterer: Option<Vec<String>>,
    penciler: Option<Vec<String>>,
    colorist: Option<Vec<String>>,
    inker: Option<Vec<String>>,
    contributor: Option<Vec<String>>,
}

impl RoleContributors {
    fn push(&mut self, entry: &ManifestContributor) {
        let target = match entry.role.trim().to_ascii_lowercase().as_str() {
            "author" => &mut self.author,
            "translator" => &mut self.translator,
            "editor" => &mut self.editor,
            "artist" => &mut self.artist,
            "illustrator" => &mut self.illustrator,
            "letterer" => &mut self.letterer,
            "penciler" | "penciller" => &mut self.penciler,
            "colorist" => &mut self.colorist,
            "inker" => &mut self.inker,
            _ => &mut self.contributor,
        };
        target.get_or_insert_with(Vec::new).push(entry.name.clone());
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

fn render_link_item(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    item: &ManifestLinkItem,
) -> WebPubManifestLinkDto {
    WebPubManifestLinkDto {
        rel: None,
        href: render_href(headers, manifest, surface, &item.href),
        media_type: item.media_type.clone(),
        properties: None,
        dimensions: item
            .include_dimensions
            .then_some(WebPubManifestLinkDimensionsDto {
                width: item.width,
                height: item.height,
            }),
        alternate: item
            .alternate
            .iter()
            .map(|alternate| render_link_item(headers, manifest, surface, alternate))
            .collect(),
    }
}

fn render_navigation_items(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    items: &[ManifestNavigationItem],
) -> Vec<WebPubManifestNavigationDto> {
    items
        .iter()
        .map(|item| render_navigation_item(headers, manifest, surface, item))
        .collect()
}

fn render_navigation_item(
    headers: &HeaderMap,
    manifest: &PersistedManifest,
    surface: ManifestHrefSurface,
    item: &ManifestNavigationItem,
) -> WebPubManifestNavigationDto {
    WebPubManifestNavigationDto {
        title: item.title.clone(),
        href: item
            .href
            .as_ref()
            .map(|href| render_href(headers, manifest, surface, href)),
        children: render_navigation_items(headers, manifest, surface, &item.children),
    }
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
