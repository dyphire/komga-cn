use super::*;
use crate::state::DiscoveryState;

#[derive(Clone)]
pub struct PersistedBookResource {
    pub library_id: String,
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
}

#[derive(Clone, Copy)]
pub enum PersistedBookSiblingDirection {
    Previous,
    Next,
}

pub async fn resolve_book_id_for_persisted(
    app: &DiscoveryState,
    requested_book_id: &str,
) -> String {
    let Some(index) = requested_book_id
        .strip_prefix("book-")
        .and_then(|value| value.parse::<usize>().ok())
    else {
        return requested_book_id.to_string();
    };

    if index == 0 {
        return requested_book_id.to_string();
    }

    if matches!(
        load_persisted_book_resource(app, requested_book_id).await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match app.book_detail.load_book_id_by_sorted_position(index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

pub async fn load_persisted_book_resource(
    app: &DiscoveryState,
    book_id: &str,
) -> Result<Option<PersistedBookResource>, String> {
    let resource = app
        .book_detail
        .load_persisted_book_resource(book_id)
        .await?
        .map(|row| PersistedBookResource {
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: parse_csv_values(&row.sharing_labels),
        });
    Ok(resource)
}

pub(super) async fn load_persisted_book_detail(
    app: &DiscoveryState,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<BookReadModel>, String> {
    app.book_detail
        .load_persisted_book_detail(book_id, user_id)
        .await
}

pub async fn load_persisted_book_series_id(
    app: &DiscoveryState,
    book_id: &str,
) -> Result<Option<String>, String> {
    Ok(load_persisted_book_detail(app, book_id, None)
        .await?
        .map(|book| book.series_id))
}

pub(super) async fn load_persisted_book_sibling_detail(
    app: &DiscoveryState,
    book_id: &str,
    direction: PersistedBookSiblingDirection,
    user_id: Option<&str>,
) -> Result<Option<BookReadModel>, String> {
    let direction = match direction {
        PersistedBookSiblingDirection::Previous => PersistedBookSiblingDirectionRecord::Previous,
        PersistedBookSiblingDirection::Next => PersistedBookSiblingDirectionRecord::Next,
    };

    let Some(sibling_id) = app
        .book_detail
        .load_persisted_book_sibling_id(book_id, direction)
        .await?
    else {
        return Ok(None);
    };

    load_persisted_book_detail(app, &sibling_id, user_id).await
}
