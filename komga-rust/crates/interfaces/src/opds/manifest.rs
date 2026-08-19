use axum::Json;
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::AuthUser;
use komga_application::media_assets::{
    ManifestBuildOutcome, ManifestVariant, build_persisted_book_manifest,
};

use crate::contracts::media_assets::{
    WebPubManifestAuthenticationLinkDto, WebPubManifestDto, WebPubManifestLinkDto,
    WebPubManifestLinkPropertiesDto,
};
use crate::helpers::spring_error_response;
use crate::media_assets::manifest_renderer::{ManifestHrefSurface, render_manifest_payload};
use crate::request_urls::app_absolute_url;
use crate::state::OpdsState;

const OPDS_MANIFEST_CONTENT_TYPE: &str = "application/opds-publication+json";
const OPDS_AUTH_CONTENT_TYPE: &str = "application/opds-authentication+json";
const PROGRESSION_REL: &str = "http://www.cantook.com/api/progression";
const PROGRESSION_CONTENT_TYPE: &str = "application/vnd.readium.progression+json";

pub(crate) async fn opds_manifest(
    headers: HeaderMap,
    app: &OpdsState,
    book_id: &str,
    user: &AuthUser,
) -> Response {
    opds_manifest_variant(headers, app, book_id, None, user).await
}

pub(crate) async fn opds_manifest_with_profile(
    headers: HeaderMap,
    app: &OpdsState,
    book_id: &str,
    profile: &str,
    user: &AuthUser,
) -> Response {
    opds_manifest_variant(headers, app, book_id, Some(profile), user).await
}

async fn opds_manifest_variant(
    headers: HeaderMap,
    app: &OpdsState,
    book_id: &str,
    profile: Option<&str>,
    user: &AuthUser,
) -> Response {
    let Some(variant) = manifest_variant(profile) else {
        return StatusCode::NOT_FOUND.into_response();
    };

    match build_persisted_book_manifest(
        app.manifest_reader.as_ref(),
        app.manifest_content.as_ref(),
        app.manifest_metadata.as_ref(),
        user,
        book_id,
        variant,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(manifest)) => {
            let mut payload =
                match render_manifest_payload(&headers, &manifest, ManifestHrefSurface::OpdsV2) {
                    Ok(payload) => payload,
                    Err(error) => {
                        return spring_error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("render persisted OPDS manifest: {error:#}"),
                        );
                    }
                };
            adapt_manifest_payload_to_opds(
                &mut payload,
                &headers,
                manifest.book_id.as_str(),
                manifest.series_id.as_deref(),
            );
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, OPDS_MANIFEST_CONTENT_TYPE)],
                Json(payload),
            )
                .into_response()
        }
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            spring_error_response(StatusCode::BAD_REQUEST, message)
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => spring_error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("load persisted OPDS manifest: {error:#}"),
        ),
    }
}

fn manifest_variant(profile: Option<&str>) -> Option<ManifestVariant> {
    match profile {
        None => Some(ManifestVariant::Default),
        Some("epub") => Some(ManifestVariant::Epub),
        Some("pdf") => Some(ManifestVariant::Pdf),
        Some("divina") => Some(ManifestVariant::Divina),
        Some(_) => None,
    }
}

fn adapt_manifest_payload_to_opds(
    payload: &mut WebPubManifestDto,
    headers: &HeaderMap,
    book_id: &str,
    series_id: Option<&str>,
) {
    add_series_links_to_belongs_to(payload, headers, series_id);
    add_auth_properties_to_manifest_links(payload, headers);
    add_auth_properties_to_thumbnail_resources(payload, headers);
    add_progression_link(payload, headers, book_id);
}

fn add_series_links_to_belongs_to(
    payload: &mut WebPubManifestDto,
    headers: &HeaderMap,
    series_id: Option<&str>,
) {
    let Some(series_id) = series_id.filter(|value| !value.is_empty()) else {
        return;
    };
    let Some(belongs_to) = payload.metadata.belongs_to.as_mut() else {
        return;
    };

    for entry in &mut belongs_to.series {
        entry.links = Some(vec![WebPubManifestLinkDto {
            rel: None,
            href: app_absolute_url(headers, format!("/opds/v2/series/{series_id}").as_str()),
            media_type: "application/opds+json".to_string(),
            properties: None,
            dimensions: None,
            alternate: vec![],
        }]);
    }
}

fn add_auth_properties_to_manifest_links(payload: &mut WebPubManifestDto, headers: &HeaderMap) {
    for link in &mut payload.links {
        insert_auth_properties(link, headers);
    }
}

fn add_auth_properties_to_thumbnail_resources(
    payload: &mut WebPubManifestDto,
    headers: &HeaderMap,
) {
    for resource in &mut payload.resources {
        let is_thumbnail = resource.href.ends_with("/thumbnail");
        if is_thumbnail {
            insert_auth_properties(resource, headers);
        }
    }
}

fn insert_auth_properties(value: &mut WebPubManifestLinkDto, headers: &HeaderMap) {
    value.properties = Some(WebPubManifestLinkPropertiesDto {
        authenticate: WebPubManifestAuthenticationLinkDto {
            href: app_absolute_url(headers, "/opds/v2/auth"),
            media_type: OPDS_AUTH_CONTENT_TYPE.to_string(),
        },
    });
}

fn add_progression_link(payload: &mut WebPubManifestDto, headers: &HeaderMap, book_id: &str) {
    if payload
        .links
        .iter()
        .any(|link| link.rel.as_deref() == Some(PROGRESSION_REL))
    {
        return;
    }
    let mut link = WebPubManifestLinkDto {
        rel: Some(PROGRESSION_REL.to_string()),
        href: app_absolute_url(
            headers,
            format!("/opds/v2/books/{book_id}/progression").as_str(),
        ),
        media_type: PROGRESSION_CONTENT_TYPE.to_string(),
        properties: None,
        dimensions: None,
        alternate: vec![],
    };
    insert_auth_properties(&mut link, headers);
    payload.links.push(link);
}
