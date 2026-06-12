use crate::discovery::{
    BookDetailPort, BookReadModel, PersistedSeriesDetailRecord, SeriesDetailPort,
    SeriesReadingDirection,
};
use crate::identity_access::{
    AuthUser, AuthUserRole, user_has_role, user_query_restrictions, user_shared_all_libraries,
    user_shared_library_ids,
};
use crate::media_assets::{
    BookAccessRestrictions, BookMediaPort, BookMediaRecord, BookPageRecord, ContentAccessPort,
    ContentResolverPort, EpubNavigationContentPort, EpubNavigationExtensionReaderPort,
    EpubNavigationLink, EpubNavigationLoadError, ManifestBookRecord, PersistedMediaFileRecord,
    load_book_epub_navigation_extension,
};
use async_trait::async_trait;
use komga_domain::discovery::{QueryRestrictions, content_allowed_by_restrictions};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestProfile {
    Epub,
    Pdf,
    Divina,
}

struct EpubNavigationHref<'a> {
    resource_path: &'a str,
    fragment: Option<&'a str>,
}

impl<'a> EpubNavigationHref<'a> {
    fn parse(href: &'a str) -> Self {
        match href.split_once('#') {
            Some((resource_path, fragment)) => Self {
                resource_path,
                fragment: Some(fragment),
            },
            None => Self {
                resource_path: href,
                fragment: None,
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestVariant {
    Default,
    Epub,
    Pdf,
    Divina,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManifestContributor {
    pub name: String,
    pub role: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManifestSeriesMetadata {
    pub name: String,
    pub position: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManifestReadingProgression {
    LeftToRight,
    RightToLeft,
    TopToBottom,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ManifestMetadata {
    pub description: Option<String>,
    pub isbn: Option<String>,
    pub number_of_pages: Option<u32>,
    pub published: Option<String>,
    pub modified: Option<String>,
    pub subjects: Vec<String>,
    pub contributors: Vec<ManifestContributor>,
    pub series: Option<ManifestSeriesMetadata>,
    pub language: Option<String>,
    pub reading_progression: Option<ManifestReadingProgression>,
    pub fixed_layout: Option<bool>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ManifestHref {
    Manifest,
    File,
    Thumbnail,
    DivinaManifest,
    Resource(String),
    RawPage(u64),
    Page(u64),
    PageJpeg(u64),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManifestLinkItem {
    pub href: ManifestHref,
    pub media_type: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub include_dimensions: bool,
    pub alternate: Vec<ManifestLinkItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ManifestNavigationItem {
    pub title: Option<String>,
    pub href: Option<ManifestHref>,
    pub children: Vec<ManifestNavigationItem>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PersistedManifest {
    pub book_id: String,
    pub title: String,
    pub media_type: String,
    pub variant: ManifestVariant,
    pub profile: ManifestProfile,
    pub metadata: ManifestMetadata,
    pub reading_order: Vec<ManifestLinkItem>,
    pub resources: Vec<ManifestLinkItem>,
    pub toc: Vec<ManifestNavigationItem>,
    pub landmarks: Vec<ManifestNavigationItem>,
    pub page_list: Vec<ManifestNavigationItem>,
    pub epub_divina_compatible: bool,
    pub series_id: Option<String>,
}

#[async_trait]
pub trait ManifestReaderPort: EpubNavigationExtensionReaderPort + Send + Sync {
    async fn manifest_book(&self, book_id: &str) -> Result<Option<ManifestBookRecord>, String>;

    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String>;

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<BookAccessRestrictions>, String>;

    async fn book_pages(&self, book_id: &str) -> Result<Vec<BookPageRecord>, String>;

    async fn media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String>;
}

#[async_trait]
pub trait ManifestContentPort: EpubNavigationContentPort {
    async fn archive_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Option<Vec<BookPageRecord>>, String>;

    fn generated_pdf_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Vec<BookPageRecord>, String>;
}

#[async_trait]
impl<T> ManifestContentPort for T
where
    T: ContentResolverPort + Send + Sync + ?Sized,
{
    async fn archive_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Option<Vec<BookPageRecord>>, String> {
        ContentResolverPort::archive_page_rows(self, media).await
    }

    fn generated_pdf_page_rows(
        &self,
        media: &BookMediaRecord,
    ) -> Result<Vec<BookPageRecord>, String> {
        ContentResolverPort::generated_pdf_page_rows(self, media)
    }
}

#[async_trait]
pub trait ManifestMetadataPort: Send + Sync {
    async fn manifest_book_detail(&self, book_id: &str) -> Result<Option<BookReadModel>, String>;

    async fn manifest_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String>;
}

#[async_trait]
impl<T> ManifestMetadataPort for T
where
    T: BookDetailPort + SeriesDetailPort + Send + Sync + ?Sized,
{
    async fn manifest_book_detail(&self, book_id: &str) -> Result<Option<BookReadModel>, String> {
        BookDetailPort::load_persisted_book_detail(self, book_id, None).await
    }

    async fn manifest_series_detail(
        &self,
        series_id: &str,
    ) -> Result<Option<PersistedSeriesDetailRecord>, String> {
        SeriesDetailPort::load_persisted_series_detail(self, series_id).await
    }
}

#[async_trait]
impl<T> ManifestReaderPort for T
where
    T: BookMediaPort + ContentAccessPort + Send + Sync + ?Sized,
{
    async fn manifest_book(&self, book_id: &str) -> Result<Option<ManifestBookRecord>, String> {
        ContentAccessPort::manifest_book(self, book_id).await
    }

    async fn book_media(&self, book_id: &str) -> Result<Option<BookMediaRecord>, String> {
        BookMediaPort::book_media(self, book_id).await
    }

    async fn book_restrictions(
        &self,
        book_id: &str,
    ) -> Result<Option<BookAccessRestrictions>, String> {
        ContentAccessPort::book_restrictions(self, book_id).await
    }

    async fn book_pages(&self, book_id: &str) -> Result<Vec<BookPageRecord>, String> {
        BookMediaPort::book_pages(self, book_id).await
    }

    async fn media_file_records(
        &self,
        book_id: &str,
    ) -> Result<Vec<PersistedMediaFileRecord>, String> {
        BookMediaPort::media_file_records(self, book_id).await
    }
}

pub enum ManifestBuildOutcome {
    Found(Box<PersistedManifest>),
    BadRequest(String),
    NotFound,
    Forbidden,
}

struct ManifestUserContext {
    allowed_library_ids: Option<Vec<String>>,
    restrictions: QueryRestrictions,
}

struct WebpubMetadataAdditions {
    metadata: ManifestMetadata,
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

        Self {
            allowed_library_ids,
            restrictions: user_query_restrictions(user),
        }
    }

    fn can_access_library(&self, library_id: &str) -> bool {
        match &self.allowed_library_ids {
            None => true,
            Some(ids) => ids.iter().any(|id| id == library_id),
        }
    }

    fn content_allowed(&self, age_rating: Option<u32>, sharing_labels: &[String]) -> bool {
        content_allowed_by_restrictions(&self.restrictions, age_rating, sharing_labels)
    }
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

fn link_item(href: ManifestHref, media_type: impl Into<String>) -> ManifestLinkItem {
    ManifestLinkItem {
        href,
        media_type: media_type.into(),
        width: None,
        height: None,
        include_dimensions: false,
        alternate: Vec::new(),
    }
}

fn epub_sub_type_matches(entry: &PersistedMediaFileRecord, expected: &str) -> bool {
    entry
        .sub_type
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn epub_link(entry: &PersistedMediaFileRecord) -> ManifestLinkItem {
    link_item(
        ManifestHref::Resource(entry.file_name.trim_start_matches('/').to_string()),
        entry.media_type.clone(),
    )
}

fn epub_nav_href(href: &str) -> ManifestHref {
    let href = EpubNavigationHref::parse(href);
    let mut resource = href.resource_path.trim_start_matches('/').to_string();
    if let Some(fragment) = href.fragment
        && !fragment.is_empty()
    {
        resource.push('#');
        resource.push_str(fragment);
    }
    ManifestHref::Resource(resource)
}

fn epub_nav_link(entry: &EpubNavigationLink) -> ManifestNavigationItem {
    ManifestNavigationItem {
        title: entry.title.clone(),
        href: entry.href.as_deref().map(epub_nav_href),
        children: entry.children.iter().map(epub_nav_link).collect(),
    }
}

fn epub_nav_links(entries: &[EpubNavigationLink]) -> Vec<ManifestNavigationItem> {
    entries.iter().map(epub_nav_link).collect()
}

fn persisted_epub_manifest_parts(
    media_type: &str,
    media_files: &[PersistedMediaFileRecord],
) -> (Vec<ManifestLinkItem>, Vec<ManifestLinkItem>) {
    let reading_order = media_files
        .iter()
        .filter(|entry| epub_sub_type_matches(entry, "EPUB_PAGE"))
        .map(epub_link)
        .collect::<Vec<_>>();
    let mut resources = vec![link_item(ManifestHref::Thumbnail, "image/jpeg")];
    resources.extend(
        media_files
            .iter()
            .filter(|entry| epub_sub_type_matches(entry, "EPUB_ASSET"))
            .map(epub_link),
    );

    (
        if reading_order.is_empty() {
            vec![default_reading_order_entry(media_type)]
        } else {
            reading_order
        },
        resources,
    )
}

async fn build_manifest_reading_order(
    reader: &dyn ManifestReaderPort,
    content: &dyn ManifestContentPort,
    book_id: &str,
    media: &BookMediaRecord,
    media_type: &str,
    variant: ManifestVariant,
    profile: ManifestProfile,
) -> Result<Vec<ManifestLinkItem>, String> {
    Ok(match (variant, profile) {
        (ManifestVariant::Pdf, ManifestProfile::Pdf) => (1..=media.page_count.max(1))
            .map(|page| link_item(ManifestHref::RawPage(page), "application/pdf"))
            .collect::<Vec<_>>(),
        (ManifestVariant::Divina, ManifestProfile::Pdf)
        | (ManifestVariant::Divina, ManifestProfile::Divina) => {
            let persisted_rows = reader.book_pages(book_id).await?;
            let effective_rows = if !persisted_rows.is_empty() {
                reading_order_entries(
                    persisted_rows,
                    (profile == ManifestProfile::Pdf).then_some("image/jpeg"),
                )
            } else if let Some(archive_rows) = content.archive_page_rows(media).await? {
                reading_order_entries(archive_rows, None)
            } else {
                reading_order_entries(content.generated_pdf_page_rows(media)?, None)
            };

            if effective_rows.is_empty() {
                vec![default_reading_order_entry(media_type)]
            } else {
                effective_rows
            }
        }
        _ => vec![default_reading_order_entry(media_type)],
    })
}

fn reading_order_entries(
    page_rows: Vec<BookPageRecord>,
    media_type_override: Option<&str>,
) -> Vec<ManifestLinkItem> {
    const RECOMMENDED_IMAGE_MEDIA_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif"];

    page_rows
        .into_iter()
        .map(|page| {
            let effective_media_type = media_type_override.unwrap_or(page.media_type.as_str());
            let mut entry = ManifestLinkItem {
                href: ManifestHref::Page(page.number),
                media_type: effective_media_type.to_string(),
                width: page.width,
                height: page.height,
                include_dimensions: true,
                alternate: Vec::new(),
            };

            if effective_media_type.starts_with("image/")
                && !RECOMMENDED_IMAGE_MEDIA_TYPES.contains(&effective_media_type)
            {
                entry.alternate.push(ManifestLinkItem {
                    href: ManifestHref::PageJpeg(page.number),
                    media_type: "image/jpeg".to_string(),
                    width: page.width,
                    height: page.height,
                    include_dimensions: true,
                    alternate: Vec::new(),
                });
            }

            entry
        })
        .collect()
}

fn default_reading_order_entry(media_type: &str) -> ManifestLinkItem {
    link_item(ManifestHref::Page(1), media_type.to_string())
}

async fn load_persisted_webpub_metadata_additions(
    metadata: &dyn ManifestMetadataPort,
    book_id: &str,
) -> Result<Option<WebpubMetadataAdditions>, String> {
    let Some(book) = metadata.manifest_book_detail(book_id).await? else {
        return Ok(None);
    };
    let series_id = book.series_id.clone();
    let series = metadata.manifest_series_detail(&book.series_id).await?;

    let mut manifest_metadata = ManifestMetadata {
        description: (!book.metadata_summary.is_empty()).then_some(book.metadata_summary),
        isbn: (!book.metadata_isbn.is_empty()).then_some(book.metadata_isbn),
        number_of_pages: (book.media_pages_count > 0).then_some(book.media_pages_count),
        published: book.metadata_release_date.filter(|value| !value.is_empty()),
        modified: (!book.last_modified.is_empty()).then_some(book.last_modified),
        subjects: book.metadata_tags,
        contributors: book
            .metadata_authors
            .into_iter()
            .filter(|author| !author.name.is_empty())
            .map(|author| ManifestContributor {
                name: author.name,
                role: author.role,
            })
            .collect(),
        series: (!book.series_title.is_empty()).then_some(ManifestSeriesMetadata {
            name: book.series_title,
            position: book
                .metadata_number_sort
                .is_finite()
                .then_some(book.metadata_number_sort),
        }),
        language: None,
        reading_progression: None,
        fixed_layout: None,
    };

    if let Some(series) = series {
        if !series.language.is_empty() {
            manifest_metadata.language = Some(series.language);
        }
        manifest_metadata.reading_progression = series
            .reading_direction
            .and_then(webpub_reading_progression);
    }

    Ok(Some(WebpubMetadataAdditions {
        metadata: manifest_metadata,
        epub_divina_compatible: book.media_epub_divina_compatible,
        series_id,
    }))
}

fn webpub_reading_progression(
    reading_direction: SeriesReadingDirection,
) -> Option<ManifestReadingProgression> {
    match reading_direction {
        SeriesReadingDirection::LeftToRight => Some(ManifestReadingProgression::LeftToRight),
        SeriesReadingDirection::RightToLeft => Some(ManifestReadingProgression::RightToLeft),
        SeriesReadingDirection::Vertical | SeriesReadingDirection::Webtoon => {
            Some(ManifestReadingProgression::TopToBottom)
        }
    }
}

pub async fn build_persisted_book_manifest(
    reader: &dyn ManifestReaderPort,
    content: &dyn ManifestContentPort,
    metadata: &dyn ManifestMetadataPort,
    user: &AuthUser,
    book_id: &str,
    variant: ManifestVariant,
) -> Result<ManifestBuildOutcome, String> {
    let Some(manifest_book) = reader.manifest_book(book_id).await? else {
        return Ok(ManifestBuildOutcome::NotFound);
    };
    let library_id = manifest_book.library_id;
    let title = manifest_book.title;
    let media_type = manifest_book.media_type;

    if !user_has_role(user, AuthUserRole::PageStreaming) {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

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
    if let Some(restrictions) = reader.book_restrictions(book_id).await?
        && !user.content_allowed(restrictions.age_rating, &restrictions.labels)
    {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let profile = manifest_profile_from_media_type(&media_type);
    let webpub_additions = load_persisted_webpub_metadata_additions(metadata, book_id).await?;
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
        let (reading_order, resources) =
            persisted_epub_manifest_parts(&media_type, media_files.as_slice());
        let mut metadata = webpub_additions
            .as_ref()
            .map(|additions| additions.metadata.clone())
            .unwrap_or_default();
        let (toc, landmarks, page_list) =
            match load_book_epub_navigation_extension(reader, content, book_id).await {
                Ok(extension) => {
                    metadata.fixed_layout = Some(extension.is_fixed_layout);
                    (
                        epub_nav_links(&extension.toc),
                        epub_nav_links(&extension.landmarks),
                        epub_nav_links(&extension.page_list),
                    )
                }
                Err(EpubNavigationLoadError::MissingExtension) => {
                    (Vec::new(), Vec::new(), Vec::new())
                }
                Err(EpubNavigationLoadError::Internal(error)) => return Err(error),
            };

        return Ok(ManifestBuildOutcome::Found(Box::new(PersistedManifest {
            book_id: book_id.to_string(),
            title,
            media_type,
            variant: effective_variant,
            profile: effective_profile,
            metadata,
            reading_order,
            resources,
            toc,
            landmarks,
            page_list,
            epub_divina_compatible,
            series_id: webpub_additions.map(|additions| additions.series_id),
        })));
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

    Ok(ManifestBuildOutcome::Found(Box::new(PersistedManifest {
        book_id: book_id.to_string(),
        title,
        media_type: media_type.clone(),
        variant: effective_variant,
        profile: effective_profile,
        metadata: webpub_additions
            .as_ref()
            .map(|additions| additions.metadata.clone())
            .unwrap_or_default(),
        reading_order,
        resources: vec![link_item(ManifestHref::Thumbnail, "image/jpeg")],
        toc: Vec::new(),
        landmarks: Vec::new(),
        page_list: Vec::new(),
        epub_divina_compatible,
        series_id: webpub_additions.map(|additions| additions.series_id),
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity_access::{AuthUser, AuthUserAgeRestriction, AuthUserAgeRestrictionKind};

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
                restriction: AuthUserAgeRestrictionKind::Exclude,
            }),
        };

        let context = ManifestUserContext::from_auth_user(&user);

        assert!(context.can_access_library("library-1"));
        assert!(!context.can_access_library("library-2"));
        assert!(context.content_allowed(Some(12), &[]));
        assert!(!context.content_allowed(Some(18), &[]));
        assert!(!context.content_allowed(None, &["Blocked".to_string()]));
    }

    #[test]
    fn webpub_reading_progression_uses_application_reading_direction() {
        assert_eq!(
            webpub_reading_progression(SeriesReadingDirection::LeftToRight),
            Some(ManifestReadingProgression::LeftToRight)
        );
        assert_eq!(
            webpub_reading_progression(SeriesReadingDirection::RightToLeft),
            Some(ManifestReadingProgression::RightToLeft)
        );
        assert_eq!(
            webpub_reading_progression(SeriesReadingDirection::Webtoon),
            Some(ManifestReadingProgression::TopToBottom)
        );
    }
}
