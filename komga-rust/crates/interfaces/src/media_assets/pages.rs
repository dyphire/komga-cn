use super::*;
use crate::identity_access::auth::Authenticated;
use crate::media_responses::{self, BookPageResponseOptions};
use crate::opds::content_opds::OpdsV1Authenticated;
use crate::state::MediaAssetsState;
use axum::extract::State;

pub async fn book_page(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Query(query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    media_responses::book_page_response(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.book_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number,
        query.into_response_options(),
    )
    .await
}

pub async fn book_page_opds_v1(
    State(app): State<MediaAssetsState>,
    OpdsV1Authenticated(user): OpdsV1Authenticated,
    headers: HeaderMap,
    Query(mut query): Query<BookPageQuery>,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    query.zero_based = true;
    query.content_negotiation = false;
    media_responses::book_page_response(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.book_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number,
        query.into_response_options(),
    )
    .await
}

pub async fn book_page_raw(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, page_number_signed)): Path<(String, i32)>,
) -> Response {
    media_responses::book_page_raw_response(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.book_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number_signed,
    )
    .await
}

#[derive(Deserialize, Default)]
pub struct BookPageQuery {
    #[serde(default)]
    pub(crate) convert: Option<String>,

    #[serde(default)]
    pub(crate) zero_based: bool,

    #[serde(default = "book_page_content_negotiation_default")]
    #[serde(rename = "contentNegotiation")]
    pub(crate) content_negotiation: bool,
}

impl BookPageQuery {
    pub(crate) fn into_response_options(self) -> BookPageResponseOptions {
        BookPageResponseOptions {
            convert: self.convert,
            zero_based: self.zero_based,
            content_negotiation: self.content_negotiation,
        }
    }
}

fn book_page_content_negotiation_default() -> bool {
    true
}

pub async fn book_page_thumbnail(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path((book_id, page_number)): Path<(String, u32)>,
) -> Response {
    media_responses::book_page_thumbnail_response(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.book_detail.as_ref(),
        &user,
        &headers,
        &book_id,
        page_number,
    )
    .await
}

pub async fn book_pages(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    Path(book_id): Path<String>,
) -> Response {
    media_responses::book_pages_response(
        app.reader.as_ref(),
        app.content.as_ref(),
        app.book_detail.as_ref(),
        &user,
        &book_id,
    )
    .await
}

pub async fn book_positions(
    State(app): State<MediaAssetsState>,
    Authenticated(user): Authenticated,
    headers: HeaderMap,
    Path(book_id): Path<String>,
) -> Response {
    let resolved_book_id = resolve_book_id_for_persisted(&app, &book_id).await;

    let media = match load_persisted_book_media_from_services(&app, &resolved_book_id).await {
        Ok(Some(media)) => media,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => return internal_error_response(error),
    };

    if !user_can_access_book_media(app.reader.as_ref(), &resolved_book_id, &user, &media).await {
        return StatusCode::FORBIDDEN.into_response();
    }

    if !book_media_is_epub(&media) {
        return StatusCode::NOT_FOUND.into_response();
    }

    let last_modified = file_last_modified_header_value(media.file_path.as_path());

    match load_persisted_epub_positions(&app, &resolved_book_id).await {
        Ok(Some(positions)) if !positions.is_empty() => {
            if let Some(last_modified) = last_modified.as_deref()
                && if_modified_since_matches(&headers, last_modified)
            {
                return asset_not_modified_response(None, Some(last_modified));
            }
            let mut response = Json(json!({
                "total": positions.len(),
                "positions": positions,
            }))
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
        Err(error) => internal_error_response(error),
    }
}
