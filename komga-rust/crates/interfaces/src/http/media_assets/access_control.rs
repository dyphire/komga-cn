use super::*;
use crate::discovery_detail_access::collections::{
    load_persisted_collection_series_ids, load_series_library_id, load_series_restrictions,
};
use crate::opds_persisted_access::load_readlist_books;

pub(super) fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

pub(super) async fn user_can_access_book_media(
    database_file: &FsPath,
    book_id: &str,
    user: &AuthUser,
    media: &PersistedBookMedia,
) -> bool {
    if !user_can_access_library(user, &media.library_id) {
        return false;
    }

    let Ok(Some((age_rating, labels))) = load_book_restrictions(database_file, book_id).await
    else {
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
    database_file: &FsPath,
    series_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    let Some(library_id) = load_series_library_id(database_file, series_id).await? else {
        return Ok(false);
    };
    if !user_can_access_library(user, &library_id) {
        return Ok(false);
    }

    let restrictions = load_series_restrictions(database_file, series_id).await?;
    Ok(principal_allows_content(
        user,
        restrictions.age_rating,
        &restrictions.labels,
    ))
}

pub(super) async fn user_can_access_readlist_media(
    database_file: &FsPath,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    let books = load_readlist_books(database_file, readlist_id).await?;
    Ok(books.iter().all(|book| {
        user_can_access_library(user, &book.library_id)
            && principal_allows_content(user, book.age_rating, &book.sharing_labels)
    }))
}

pub(super) async fn user_can_access_collection_media(
    database_file: &FsPath,
    collection_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    let series_ids = load_persisted_collection_series_ids(database_file, collection_id).await?;
    for series_id in series_ids {
        if !user_can_access_series_media(database_file, &series_id, user).await? {
            return Ok(false);
        }
    }

    Ok(true)
}
