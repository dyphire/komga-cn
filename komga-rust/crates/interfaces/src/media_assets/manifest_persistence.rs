use super::*;
use crate::discovery::detail::load_persisted_webpub_metadata_additions;
use flate2::read::GzDecoder;
use komga_application::media_assets::BookPageRecord;
use std::io::Read;

const EPUB_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/epub";
const PDF_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/pdf";
const DIVINA_PROFILE_URL: &str = "https://readium.org/webpub-manifest/profiles/divina";

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

pub(super) fn persisted_manifest_payload(
    headers: &HeaderMap,
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
                "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/manifest").as_str()),
                "type": manifest_type,
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

fn profile_conforms_to(profile: ManifestProfile) -> &'static str {
    match profile {
        ManifestProfile::Epub => EPUB_PROFILE_URL,
        ManifestProfile::Pdf => PDF_PROFILE_URL,
        ManifestProfile::Divina => DIVINA_PROFILE_URL,
    }
}

fn manifest_extra_links(
    headers: &HeaderMap,
    book_id: &str,
    profile: ManifestProfile,
    epub_divina_compatible: bool,
) -> Vec<Value> {
    match profile {
        ManifestProfile::Pdf => vec![json!({
            "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/manifest/divina").as_str()),
            "type": "application/divina+json",
        })],
        ManifestProfile::Epub if epub_divina_compatible => vec![json!({
            "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/manifest/divina").as_str()),
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
    headers: &HeaderMap,
    book_id: &str,
    profile: ManifestProfile,
    epub_divina_compatible: bool,
) {
    let extra_links = manifest_extra_links(headers, book_id, profile, epub_divina_compatible);
    if extra_links.is_empty() {
        return;
    }
    if let Some(links) = payload.get_mut("links").and_then(Value::as_array_mut) {
        links.extend(extra_links);
    }
}

fn epub_resource_href(headers: &HeaderMap, book_id: &str, file_name: &str) -> String {
    app_absolute_url(
        headers,
        format!(
            "/api/v1/books/{book_id}/resource/{}",
            file_name.trim_start_matches('/')
        )
        .as_str(),
    )
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

fn epub_link(entry: &PersistedMediaFileRecord, headers: &HeaderMap, book_id: &str) -> Value {
    json!({
        "href": epub_resource_href(headers, book_id, entry.file_name.as_str()),
        "type": entry.media_type,
    })
}

fn epub_sub_type_matches(entry: &PersistedMediaFileRecord, expected: &str) -> bool {
    entry
        .sub_type
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case(expected))
}

fn epub_nav_href(headers: &HeaderMap, book_id: &str, href: &str) -> String {
    let (path, fragment) = href
        .split_once('#')
        .map_or((href, None), |(path, fragment)| (path, Some(fragment)));
    let mut absolute = epub_resource_href(headers, book_id, path);
    if let Some(fragment) = fragment
        && !fragment.is_empty()
    {
        absolute.push('#');
        absolute.push_str(fragment);
    }
    absolute
}

fn epub_nav_link(headers: &HeaderMap, book_id: &str, entry: &Value) -> Value {
    let mut link = serde_json::Map::new();
    if let Some(title) = entry.get("title").cloned() {
        link.insert("title".to_string(), title);
    }
    if let Some(href) = entry.get("href").and_then(Value::as_str) {
        link.insert(
            "href".to_string(),
            Value::String(epub_nav_href(headers, book_id, href)),
        );
    }
    let children = entry
        .get("children")
        .and_then(Value::as_array)
        .map(|children| {
            children
                .iter()
                .map(|child| epub_nav_link(headers, book_id, child))
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
    headers: &HeaderMap,
    book_id: &str,
) -> Vec<Value> {
    extension_payload
        .and_then(|payload| payload.get(field_name))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .map(|entry| epub_nav_link(headers, book_id, entry))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
fn persisted_epub_manifest_payload(
    headers: &HeaderMap,
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
        .map(|entry| epub_link(entry, headers, book_id))
        .collect::<Vec<_>>();
    let extension_payload = extension_blob
        .map(decode_epub_extension_payload)
        .transpose()?;
    let resources = media_files
        .iter()
        .filter(|entry| epub_sub_type_matches(entry, "EPUB_ASSET"))
        .map(|entry| epub_link(entry, headers, book_id))
        .collect::<Vec<_>>();

    let mut payload = persisted_manifest_payload(
        headers,
        book_id,
        title,
        media_type,
        manifest_content_type(ManifestVariant::Epub, ManifestProfile::Epub),
        if reading_order.is_empty() {
            vec![default_reading_order_entry(headers, book_id, media_type)]
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
            Value::Array(epub_nav_links(
                extension_payload.as_ref(),
                "toc",
                headers,
                book_id,
            )),
        );
        payload_map.insert(
            "landmarks".to_string(),
            Value::Array(epub_nav_links(
                extension_payload.as_ref(),
                "landmarks",
                headers,
                book_id,
            )),
        );
        payload_map.insert(
            "pageList".to_string(),
            Value::Array(epub_nav_links(
                extension_payload.as_ref(),
                "pageList",
                headers,
                book_id,
            )),
        );
    }

    append_manifest_extra_links(
        &mut payload,
        headers,
        book_id,
        ManifestProfile::Epub,
        epub_divina_compatible,
    );

    Ok(payload)
}

async fn build_manifest_reading_order(
    app: &HttpAppState,
    headers: &HeaderMap,
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
            let persisted_rows = load_persisted_book_pages_from_services(app, book_id).await?;
            let effective_rows = if !persisted_rows.is_empty() {
                reading_order_entries(
                    headers,
                    book_id,
                    persisted_rows,
                    (profile == ManifestProfile::Pdf).then_some("image/jpeg"),
                )
            } else if let Some(archive_rows) = load_archive_page_rows_from_services(app, media).await {
                reading_order_entries(headers, book_id, archive_rows, None)
            } else {
                reading_order_entries(headers, book_id, load_generated_pdf_page_rows_from_services(app, media), None)
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
    const RECOMMENDED_IMAGE_MEDIA_TYPES: &[&str] = &["image/jpeg", "image/png", "image/gif"];

    page_rows
        .into_iter()
        .map(|page| {
            let effective_media_type = media_type_override.unwrap_or(page.media_type.as_str());
            let mut entry = serde_json::Map::from_iter([
                (
                    "href".to_string(),
                    Value::String(app_absolute_url(
                        headers,
                        format!(
                            "/api/v1/books/{book_id}/pages/{}?contentNegotiation=false",
                            page.number
                        )
                        .as_str(),
                    )),
                ),
                (
                    "type".to_string(),
                    Value::String(effective_media_type.to_string()),
                ),
                ("width".to_string(), page.width.map_or(Value::Null, Value::from)),
                ("height".to_string(), page.height.map_or(Value::Null, Value::from)),
            ]);

            if effective_media_type.starts_with("image/")
                && !RECOMMENDED_IMAGE_MEDIA_TYPES.contains(&effective_media_type)
            {
                entry.insert(
                    "alternate".to_string(),
                    Value::Array(vec![json!({
                        "href": app_absolute_url(
                            headers,
                            format!(
                                "/api/v1/books/{book_id}/pages/{}?contentNegotiation=false&convert=jpeg",
                                page.number
                            )
                            .as_str(),
                        ),
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

fn default_reading_order_entry(headers: &HeaderMap, book_id: &str, media_type: &str) -> Value {
    json!({
        "href": app_absolute_url(headers, format!("/api/v1/books/{book_id}/pages/1?contentNegotiation=false").as_str()),
        "type": media_type,
    })
}

pub(crate) async fn build_persisted_book_manifest(
    app: &HttpAppState,
    headers: &HeaderMap,
    book_id: &str,
    variant: ManifestVariant,
) -> Result<ManifestBuildOutcome, String> {
    let Some(user) = resolved_request_auth_user(&*app.services.runtime_identity, headers).await
    else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    let Some((library_id, title, media_type)) =
        load_persisted_manifest_book_from_services(app, book_id).await?
    else {
        return Ok(ManifestBuildOutcome::NotFound);
    };

    if !user_can_access_library(&user, &library_id) {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let Some(media) = load_persisted_book_media_from_services(app, book_id).await? else {
        return Ok(ManifestBuildOutcome::NotFound);
    };
    if !user_can_access_book_media(app, book_id, &user, &media).await {
        return Ok(ManifestBuildOutcome::Forbidden);
    }

    let profile = manifest_profile_from_media_type(&media_type);
    let webpub_additions = load_persisted_webpub_metadata_additions(app, book_id).await?;
    let epub_divina_compatible = webpub_additions
        .as_ref()
        .is_some_and(|(_, epub_divina_compatible)| *epub_divina_compatible);
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
        let media_files = load_persisted_media_file_records_from_services(app, book_id).await?;
        let extension_blob = app
            .services
            .media_assets
            .load_persisted_epub_extension_blob(book_id.to_string())
            .await?;
        let payload = persisted_epub_manifest_payload(
            headers,
            book_id,
            &title,
            &media_type,
            media_files.as_slice(),
            extension_blob.as_ref().map(|(_, blob)| blob.as_slice()),
            webpub_additions.as_ref().map(|(metadata, _)| metadata),
            epub_divina_compatible,
        )?;
        return Ok(ManifestBuildOutcome::Found(
            manifest_content_type(effective_variant, effective_profile),
            payload,
        ));
    }

    let reading_order = build_manifest_reading_order(
        app,
        headers,
        book_id,
        &media,
        &media_type,
        effective_variant,
        effective_profile,
    )
    .await?;

    let mut payload = persisted_manifest_payload(
        headers,
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
            webpub_additions.as_ref().map(|(metadata, _)| metadata),
            effective_profile,
        );
    }
    append_manifest_extra_links(
        &mut payload,
        headers,
        book_id,
        effective_profile,
        epub_divina_compatible,
    );
    Ok(ManifestBuildOutcome::Found(
        manifest_content_type(effective_variant, effective_profile),
        payload,
    ))
}
