use super::*;
use komga_infrastructure::media_reader::MediaReader;

#[derive(Clone)]
pub(super) struct PersistedReadlistBookAccessRecord {
    pub id: String,
}

pub(super) fn user_can_access_library(user: &AuthUser, library_id: &str) -> bool {
    user_shared_all_libraries(user)
        || user_shared_library_ids(user)
            .iter()
            .any(|shared_library_id| shared_library_id == library_id)
}

pub(crate) async fn user_can_access_book_media(
    reader: &MediaReader,
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
    let Some(library_id) = app.series_access.load_series_library_id(series_id).await? else {
        return Ok(false);
    };
    if !user_can_access_library(user, &library_id) {
        return Ok(false);
    }

    let restriction_record = app
        .series_access
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
    Ok(!visible_readlist_books_for_user(app, readlist_id, user)
        .await?
        .is_empty())
}

pub(super) async fn visible_readlist_books_for_user(
    app: &MediaAssetsState,
    readlist_id: &str,
    user: &AuthUser,
) -> Result<Vec<PersistedReadlistBookAccessRecord>, String> {
    let books = app
        .readlist_access
        .load_persisted_readlist_book_rows(readlist_id)
        .await?;
    let mut visible_books = Vec::new();
    for book in books {
        if !user_can_access_library(user, &book.library_id) {
            continue;
        }

        let (age_rating, sharing_labels) = app
            .reader
            .book_restrictions(&book.book_id)
            .await
            .ok()
            .flatten()
            .unwrap_or((None, Vec::new()));
        if principal_allows_content(user, age_rating, &sharing_labels) {
            visible_books.push(PersistedReadlistBookAccessRecord { id: book.book_id });
        }
    }
    Ok(visible_books)
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
        .collection_access
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
