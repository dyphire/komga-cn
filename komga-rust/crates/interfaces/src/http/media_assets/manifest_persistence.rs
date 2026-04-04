use super::*;
use komga_application::media_assets::BookPageRecord;

pub(super) fn manifest_profile_from_media_type(media_type: &str) -> ManifestProfile {
    if media_type == "application/epub+zip" {
        ManifestProfile::Epub
    } else if media_type == "application/pdf" {
        ManifestProfile::Pdf
    } else {
        ManifestProfile::Divina
    }
}

pub(super) fn manifest_content_type(
    variant: ManifestVariant,
    profile: ManifestProfile,
) -> &'static str {
    match variant {
        ManifestVariant::Divina => "application/divina+json",
        ManifestVariant::Default => match profile {
            ManifestProfile::Divina => "application/divina+json",
            ManifestProfile::Epub | ManifestProfile::Pdf => "application/webpub+json",
        },
        ManifestVariant::Epub | ManifestVariant::Pdf => "application/webpub+json",
    }
}

pub(super) fn manifest_variant_matches_profile(
    variant: ManifestVariant,
    profile: ManifestProfile,
) -> bool {
    match variant {
        ManifestVariant::Default => true,
        ManifestVariant::Epub => profile == ManifestProfile::Epub,
        ManifestVariant::Pdf => profile == ManifestProfile::Pdf,
        ManifestVariant::Divina => {
            profile == ManifestProfile::Divina || profile == ManifestProfile::Pdf
        }
    }
}

pub(super) fn persisted_manifest_payload(
    headers: &HeaderMap,
    book_id: &str,
    title: &str,
    media_type: &str,
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
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/manifest").as_str()),
                "type": "application/webpub+json",
            },
            {
                "rel": "http://opds-spec.org/acquisition",
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/file").as_str()),
                "type": media_type,
            }
        ],
        "readingOrder": reading_order,
        "resources": [
            {
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/thumbnail").as_str()),
                "type": "image/jpeg",
            }
        ],
        "toc": [],
        "landmarks": [],
        "pageList": [],
    })
}

async fn build_manifest_reading_order(
    headers: &HeaderMap,
    database_file: &FsPath,
    book_id: &str,
    media: &PersistedBookMedia,
    media_type: &str,
    variant: ManifestVariant,
    profile: ManifestProfile,
) -> Result<Vec<Value>, String> {
    Ok(match (variant, profile) {
        (ManifestVariant::Pdf, ManifestProfile::Pdf) => (1..=media.page_count.max(1))
            .map(|page| {
                json!({
                    "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/pages/{page}/raw").as_str()),
                    "type": "application/pdf",
                })
            })
            .collect::<Vec<_>>(),
        (ManifestVariant::Divina, ManifestProfile::Pdf)
        | (ManifestVariant::Divina, ManifestProfile::Divina) => {
            let persisted_rows = load_persisted_book_pages(database_file, book_id).await?;
            let effective_rows = if !persisted_rows.is_empty() {
                reading_order_entries(
                    headers,
                    book_id,
                    persisted_rows,
                    (profile == ManifestProfile::Pdf).then_some("image/jpeg"),
                )
            } else if let Some(archive_rows) = load_archive_page_rows(media) {
                reading_order_entries(headers, book_id, archive_rows, None)
            } else {
                reading_order_entries(headers, book_id, load_generated_pdf_page_rows(media), None)
            };

            if effective_rows.is_empty() {
                vec![default_reading_order_entry(headers, book_id, media_type)]
            } else {
                effective_rows
            }
        }
        _ => vec![default_reading_order_entry(headers, book_id, media_type)],
    })
}

fn reading_order_entries(
    headers: &HeaderMap,
    book_id: &str,
    page_rows: Vec<BookPageRecord>,
    media_type_override: Option<&str>,
) -> Vec<Value> {
    page_rows
        .into_iter()
        .map(|page| {
            json!({
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/pages/{}?contentNegotiation=false", page.number).as_str()),
                "type": media_type_override.unwrap_or(page.media_type.as_str()),
                "width": page.width,
                "height": page.height,
            })
        })
        .collect()
}

fn default_reading_order_entry(headers: &HeaderMap, book_id: &str, media_type: &str) -> Value {
    json!({
        "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/pages/1?contentNegotiation=false").as_str()),
        "type": media_type,
    })
}

pub(super) async fn build_persisted_book_manifest(
    database_file: &FsPath,
    headers: &HeaderMap,
    book_id: &str,
    variant: ManifestVariant,
) -> Result<ManifestBuildOutcome, String> {
    let Some(user) = resolved_auth_user(headers) else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    let Some((library_id, title, media_type)) =
        load_persisted_manifest_book(database_file, book_id).await?
    else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    if !user_can_access_library(&user, &library_id) {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let Some(media) = load_persisted_book_media(database_file, book_id).await? else {
        return Ok(ManifestBuildOutcome::NotFound);
    };
    if !user_can_access_book_media(database_file, book_id, &user, &media).await {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let profile = manifest_profile_from_media_type(&media_type);
    if !manifest_variant_matches_profile(variant, profile) {
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

    let reading_order = build_manifest_reading_order(
        headers,
        database_file,
        book_id,
        &media,
        &media_type,
        variant,
        profile,
    )
    .await?;

    let payload = persisted_manifest_payload(headers, book_id, &title, &media_type, reading_order);
    Ok(ManifestBuildOutcome::Found(
        manifest_content_type(variant, profile),
        payload,
    ))
}
