use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::identity_access::AuthUser;

use crate::helpers::spring_error_response;
use crate::identity_access::auth::Authenticated;
use crate::media_assets::http_helpers::internal_error_response;
use crate::media_assets::manifest_renderer::{
    ManifestHrefSurface, manifest_content_type, render_manifest_payload,
};
use crate::state::MediaAssetsState;
use komga_application::media_assets::{
    ManifestBuildOutcome, ManifestVariant, build_persisted_book_manifest,
};

pub(crate) async fn book_manifest(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Default).await
}

pub(crate) async fn book_manifest_epub(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Epub).await
}

pub(crate) async fn book_manifest_pdf(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Pdf).await
}

pub(crate) async fn book_manifest_divina(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Divina).await
}

async fn book_manifest_variant(
    app: MediaAssetsState,
    user: AuthUser,
    headers: HeaderMap,
    book_id: String,
    variant: ManifestVariant,
) -> Response {
    match build_persisted_book_manifest(
        app.manifest_reader.as_ref(),
        app.manifest_content.as_ref(),
        app.manifest_metadata.as_ref(),
        &user,
        &book_id,
        variant,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(manifest)) => {
            let payload =
                match render_manifest_payload(&headers, &manifest, ManifestHrefSurface::ApiV1) {
                    Ok(payload) => payload,
                    Err(error) => return internal_error_response(error),
                };
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, manifest_content_type(&manifest))],
                Json(payload),
            )
                .into_response()
        }
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            spring_error_response(StatusCode::BAD_REQUEST, message)
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}
