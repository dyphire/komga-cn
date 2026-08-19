use super::access_control::user_can_access_book_media;
use super::http_helpers::internal_error_response;
use super::media_helpers::book_media_is_epub;
use crate::book_page_query::BookPageQuery;
use crate::cache::{
    asset_not_modified_response, file_last_modified_header_value, if_modified_since_matches,
};
use crate::contracts::media_assets::ReadiumPositionListDto;
use crate::identity_access::auth::Authenticated;
use crate::media_responses::BookMediaResponses;
use crate::opds_auth::OpdsV1Authenticated;
use crate::state::MediaAssetsState;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::resolve_persisted_book_id;
use komga_application::identity_access::{AuthUserRole, user_has_role};
use komga_application::media_assets::{EpubNavigationLoadError, load_book_epub_positions};

fn book_media_responses(app: &MediaAssetsState) -> BookMediaResponses<'_> {
    BookMediaResponses::new(
        app.book_media_reader.as_ref(),
        app.book_media_content.as_ref(),
        app.book_id_resolver.as_ref(),
    )
}

pub(crate) async fn book_page(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    book_media_responses(&app)
        .book_page(
            &user,
            &headers,
            &book_id,
            page_number,
            query.into_response_options(),
        )
        .await
}

pub(crate) async fn book_page_opds_v1(
    State(app): State<MediaAssetsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    book_media_responses(&app)
        .book_page(
            &user,
            &headers,
            &book_id,
            page_number,
            query.into_opds_v1_response_options(),
        )
        .await
}

pub(crate) async fn book_page_raw(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, page_number_signed)): Path<(String, i32)>,
) -> Response {
    book_media_responses(&app)
        .book_page_raw(&user, &headers, &book_id, page_number_signed)
        .await
}

pub(crate) async fn book_page_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    book_media_responses(&app)
        .book_page_thumbnail(&user, &headers, &book_id, page_number)
        .await
}

pub(crate) async fn book_pages(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    book_media_responses(&app).book_pages(&user, &book_id).await
}

pub(crate) async fn book_positions(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let resolved_book_id = resolve_persisted_book_id(app.book_id_resolver.as_ref(), &book_id).await;

    let media = match app.book_media_reader.book_media(&resolved_book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };
    if !user_has_role(&user, AuthUserRole::PageStreaming) {
        return StatusCode::FORBIDDEN.into_response();
    }

    match user_can_access_book_media(
        app.book_media_reader.as_ref(),
        &resolved_book_id,
        &user,
        &media,
    )
    .await
    {
        Ok(true) => {}
        Ok(false) => return StatusCode::FORBIDDEN.into_response(),
        Err(error) => return internal_error_response(error),
    }

    if !book_media_is_epub(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let last_modified = file_last_modified_header_value(media.file_path.as_path());

    match load_book_epub_positions(
        app.epub_navigation_reader.as_ref(),
        app.epub_navigation_content.as_ref(),
        &resolved_book_id,
    )
    .await
    {
        Ok(Some(positions)) if !positions.is_empty() => {
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(&headers, last_modified)
            {
                return asset_not_modified_response(None, Some(last_modified));
            }
            let mut response = Json(ReadiumPositionListDto {
                total: positions.len(),
                positions,
            })
            .into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/vnd.readium.position-list+json"),
            );
            if let Some(last_modified) = last_modified.as_deref() {
                response.headers_mut().insert(
                    header::LAST_MODIFIED,
                    HeaderValue::from_str(last_modified)
                        .expect("positions last-modified header should be valid"),
                );
            }
            response
        }
        Ok(_) => StatusCode::NOT_FOUND.into_response(),
        Err(EpubNavigationLoadError::MissingExtension) => StatusCode::NOT_FOUND.into_response(),
        Err(EpubNavigationLoadError::Internal(error)) => internal_error_response(error),
    }
}
