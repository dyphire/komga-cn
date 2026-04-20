use super::*;
use crate::discovery::detail::load_series_library_id;

#[derive(Clone)]
pub(super) struct PersistedReadlistBookAccessRecord {
    pub id: String,
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
}

pub(super) fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

pub(crate) async fn user_can_access_book_media(
    app: &HttpAppState,
    book_id: &str,
    user: &AuthUser,
    media: &PersistedBookMedia,
) -> bool {
    if !user_can_access_library(user, &media.library_id) {
        return false;
    }

    let Ok(Some((age_rating, labels))) = load_book_restrictions_from_services(app, book_id).await
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
    app: &HttpAppState,
    series_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    let Some(library_id) = load_series_library_id(app, series_id).await? else {
        return Ok(false);
    };
    if !user_can_access_library(user, &library_id) {
        return Ok(false);
    }

    let restriction_record = app
        .services
        .discovery_detail
        .load_series_restrictions(app.auth_db.database_file.clone(), series_id.to_string())
        .await?;
    Ok(principal_allows_content(
        user,
        restriction_record.age_rating,
        &restriction_record.labels,
    ))
}

pub(super) async fn user_can_access_readlist_media(
    app: &HttpAppState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<bool, String> {
    Ok(!visible_readlist_books_for_user(app, readlist_id, user)
        .await?
        .is_empty())
}

pub(super) async fn visible_readlist_books_for_user(
    app: &HttpAppState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<Vec<PersistedReadlistBookAccessRecord>, String> {
    let books = app
        .services
        .opds_persisted
        .load_readlist_books(app.auth_db.database_file.clone(), readlist_id.to_string())
        .await?;
    Ok(books
        .into_iter()
        .map(|book| PersistedReadlistBookAccessRecord {
            id: book.id,
            library_id: book.library_id,
            age_rating: book.age_rating,
            sharing_labels: book.sharing_labels,
        })
        .filter(|book| {
            user_can_access_library(user, &book.library_id)
                && principal_allows_content(user, book.age_rating, &book.sharing_labels)
        })
        .collect())
}

pub(super) async fn user_can_access_collection_media(
    app: &HttpAppState,
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
    app: &HttpAppState,
    collection_id: &str,
    user: &AuthUser,
) -> Result<Vec<String>, String> {
    let series_ids = app
        .services
        .discovery_detail
        .load_persisted_collection_series_ids(
            app.auth_db.database_file.clone(),
            collection_id.to_string(),
        )
        .await?;
    let mut visible_series_ids = Vec::new();
    for series_id in series_ids {
        if user_can_access_series_media(app, &series_id, user).await? {
            visible_series_ids.push(series_id);
        }
    }

    Ok(visible_series_ids)
}
