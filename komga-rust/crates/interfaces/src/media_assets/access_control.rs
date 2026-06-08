use super::*;
use crate::discovery_auth::context::to_query_context;
use crate::helpers::to_domain_query_context;
use komga_application::discovery::ReadlistVisibilityService;
use komga_application::media_assets::MediaReaderPort;

pub(super) fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

pub(crate) async fn user_can_access_book_media(
    reader: &dyn MediaReaderPort,
    book_id: &str,
    user: &AuthUser,
    media: &PersistedBookMedia,
) -> bool {
    if !user_can_access_library(user, &media.library_id) {
        return false;
    }

    let Ok(Some((age_rating, labels))) = reader.book_restrictions(book_id).await else {
        return true;
    };

    principal_allows_content(user, age_rating, &labels)
}

fn principal_allows_content(user: &AuthUser, age_rating: Option<u16>, labels: &[String]) -> bool {
    let payload = user_payload_json(user);
    let Some(principal) = principal_from_user_payload(&payload) else {
        return true;
    };
    if !principal.restrictions.is_restricted() {
        return true;
    }

    principal.is_content_allowed(age_rating.map(u32::from), labels)
}

pub(super) async fn user_can_access_series_media(
    app: &MediaAssetsState,
    series_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    let Some(library_id) = app.series_detail.load_series_library_id(series_id).await? else {
        return Ok(false);
    };
    if !user_can_access_library(user, &library_id) {
        return Ok(false);
    }

    let restriction_record = app
        .series_detail
        .load_series_restrictions(series_id)
        .await?;
    Ok(principal_allows_content(
        user,
        restriction_record.age_rating,
        &restriction_record.labels,
    ))
}

pub(super) async fn user_can_access_readlist_media(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    Ok(visible_readlist_book_ids_for_user(app, readlist_id, user)
        .await?
        .is_some_and(|book_ids| !book_ids.is_empty()))
}

pub(super) async fn visible_readlist_book_ids_for_user(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<Option<Vec<String>>, String> {
    let principal = principal_from_user_payload(&user_payload_json(user))
        .expect("authenticated user payload should resolve to discovery principal");
    let context = to_domain_query_context(to_query_context(&principal, None));
    ReadlistVisibilityService::new(app.readlist.as_ref(), app.book_detail.as_ref())
        .visible_readlist_book_ids(&context, readlist_id)
        .await
}

pub(super) async fn user_can_access_collection_media(
    app: &MediaAssetsState,
    collection_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    Ok(
        !visible_collection_series_ids_for_user(app, collection_id, user)
            .await?
            .is_empty(),
    )
}

pub(super) async fn visible_collection_series_ids_for_user(
    app: &MediaAssetsState,
    collection_id: &str,
    user: &AuthUser,
) -> Result<Vec<String>, String> {
    let series_ids = app
        .collection
        .load_persisted_collection_series_ids(collection_id)
        .await?;
    let mut visible_series_ids = Vec::new();
    for series_id in series_ids {
        if user_can_access_series_media(app, &series_id, user).await? {
            visible_series_ids.push(series_id);
        }
    }

    Ok(visible_series_ids)
}
