use super::*;

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

    let payload = user_payload_json(user);
    let Some(principal) = principal_from_user_payload(&payload) else {
        return true;
    };
    if !principal.restrictions.is_restricted() {
        return true;
    }

    let Ok(Some((age_rating, labels))) = load_book_restrictions(database_file, book_id).await
    else {
        return true;
    };

    principal.is_content_allowed(age_rating, &labels)
}
