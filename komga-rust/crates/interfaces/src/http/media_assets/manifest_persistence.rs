use super::*;

pub(super) fn manifest_profile_from_media_type(media_type: &str) -> ManifestProfile {
    if media_type == "application/epub+zip" {
        ManifestProfile::Epub
    } else if media_type == "application/pdf" {
        ManifestProfile::Pdf
    } else {
        ManifestProfile::Divina
    }
}

pub(super) fn manifest_content_type(profile: ManifestProfile) -> &'static str {
    match profile {
        ManifestProfile::Divina => "application/divina+json",
        ManifestProfile::Epub | ManifestProfile::Pdf => "application/webpub+json",
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
        ManifestVariant::Divina => profile == ManifestProfile::Divina,
    }
}

pub(super) fn persisted_manifest_payload(
    headers: &HeaderMap,
    book_id: &str,
    title: &str,
    media_type: &str,
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
        "readingOrder": [
            {
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/pages/1?contentNegotiation=false").as_str()),
                "type": media_type,
            }
        ],
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

    let profile = manifest_profile_from_media_type(&media_type);
    if !manifest_variant_matches_profile(variant, profile) {
        return Ok(ManifestBuildOutcome::NotFound);
    }

    let payload = persisted_manifest_payload(headers, book_id, &title, &media_type);
    Ok(ManifestBuildOutcome::Found(
        manifest_content_type(profile),
        payload,
    ))
}
