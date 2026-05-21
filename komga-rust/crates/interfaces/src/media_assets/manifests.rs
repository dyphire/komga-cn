use super::*;
use crate::identity_access::auth::Authenticated;
use crate::request_urls::absolutize_json_hrefs;
use crate::state::MediaAssetsState;
use axum::extract::State;
use komga_application::media_assets::{
    ManifestBuildOutcome, ManifestVariant, build_persisted_book_manifest,
};

pub async fn book_manifest(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Default).await
}

pub async fn book_manifest_epub(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Epub).await
}

pub async fn book_manifest_pdf(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    book_manifest_variant(app, user, headers, book_id, ManifestVariant::Pdf).await
}

pub async fn book_manifest_divina(
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
        app.reader.as_ref(),
        app.content.as_ref(),
        app.book_detail.as_ref(),
        app.series_detail.as_ref(),
        &user,
        &book_id,
        variant,
    )
    .await
    {
        Ok(ManifestBuildOutcome::Found(mut manifest)) => {
            absolutize_json_hrefs(&headers, &mut manifest.payload);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, manifest.content_type)],
                Json(manifest.payload),
            )
                .into_response()
        }
        Ok(ManifestBuildOutcome::BadRequest(message)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": message }))).into_response()
        }
        Ok(ManifestBuildOutcome::NotFound) => StatusCode::NOT_FOUND.into_response(),
        Ok(ManifestBuildOutcome::Forbidden) => StatusCode::FORBIDDEN.into_response(),
        Err(error) => internal_error_response(error),
    }
}
