use std::collections::HashSet;
use std::path::Path;

use axum::http::HeaderMap;
use serde_json::{Value, json};

use crate::http::request_urls::app_absolute_url;
use crate::opds_catalog_access::{
    OpdsBookFeedEntry, OpdsReadlistEntry, OpdsSeriesEntry,
    load_all_readlists as load_all_readlist_entries, load_browse_publisher_entries,
    load_browse_series_navigation_entries,
    load_keep_reading_books as load_keep_reading_book_entries,
    load_latest_books_paged as load_latest_book_entries_paged,
    load_latest_series_paged as load_latest_series_entries_paged,
    load_library_series as load_library_series_entries,
    load_on_deck_books as load_on_deck_book_entries, load_series_page as load_series_entries_page,
};

use super::{PersistedBookFeedItem, PersistedReadlist, PersistedSeries};

fn persisted_book_feed_item(entry: OpdsBookFeedEntry) -> PersistedBookFeedItem {
    PersistedBookFeedItem {
        id: entry.id,
        title: entry.title,
        series_title: entry.series_title,
        number: entry.number,
        summary: entry.summary,
        authors: entry
            .authors
            .into_iter()
            .map(|author| author.name)
            .collect(),
        file_name: entry.file_name,
        file_size: entry.file_size,
        media_type: entry.media_type,
        page_count: entry.page_count,
        epub_divina_compatible: entry.epub_divina_compatible,
        last_read: entry.last_read,
        last_read_date: entry.last_read_date,
        library_id: entry.library_id,
        age_rating: entry.age_rating,
        sharing_labels: entry.sharing_labels,
        last_modified: entry.last_modified,
    }
}

fn persisted_series(entry: OpdsSeriesEntry) -> PersistedSeries {
    PersistedSeries {
        id: entry.id,
        library_id: entry.library_id,
        title: entry.title,
        age_rating: entry.age_rating,
        sharing_labels: entry.sharing_labels,
        last_modified: entry.last_modified,
    }
}

fn persisted_readlist(entry: OpdsReadlistEntry) -> PersistedReadlist {
    PersistedReadlist {
        id: entry.id,
        name: entry.name,
        last_modified: entry.last_modified,
        ordered: false,
    }
}

pub(super) async fn load_browse_series_navigation(
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    publishers: &[String],
    page: usize,
    size: usize,
) -> Result<(Vec<Value>, usize), String> {
    let (entries, total) = load_browse_series_navigation_entries(
        database_file,
        allowed_library_ids,
        library_id,
        publishers,
        page,
        size,
    )
    .await?;

    Ok((
        entries
            .into_iter()
            .map(|entry| {
                json!({
                    "title": entry.title,
                    "href": app_absolute_url(headers, format!("/opds/v2/series/{}", entry.id).as_str()),
                    "type": "application/opds+json",
                })
            })
            .collect(),
        total,
    ))
}

pub(super) async fn load_browse_publisher_navigation(
    headers: &HeaderMap,
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
) -> Result<Vec<Value>, String> {
    let entries =
        load_browse_publisher_entries(database_file, allowed_library_ids, library_id).await?;
    let library_segment = library_id.map(|id| format!("/{id}")).unwrap_or_default();
    Ok(entries
        .into_iter()
        .map(|entry| {
            let href = format!(
                "/opds/v2/libraries{library_segment}/browse?publisher={}",
                super::super::feeds::query_escape(entry.publisher.as_str()),
            );
            json!({
                "title": entry.publisher,
                "href": app_absolute_url(headers, href.as_str()),
                "type": "application/opds+json",
            })
        })
        .collect())
}

pub(super) async fn load_keep_reading_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<PersistedBookFeedItem>, String> {
    load_keep_reading_book_entries(database_file, user_id, library_id)
        .await
        .map(|entries| entries.into_iter().map(persisted_book_feed_item).collect())
}

pub(super) async fn load_on_deck_books(
    database_file: &Path,
    user_id: &str,
    library_id: Option<&str>,
) -> Result<Vec<PersistedBookFeedItem>, String> {
    load_on_deck_book_entries(database_file, user_id, library_id)
        .await
        .map(|entries| entries.into_iter().map(persisted_book_feed_item).collect())
}

pub(super) async fn load_latest_books_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    user_id: Option<&str>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedBookFeedItem>, String> {
    load_latest_book_entries_paged(
        database_file,
        allowed_library_ids,
        user_id,
        library_id,
        offset,
        limit,
    )
    .await
    .map(|entries| entries.into_iter().map(persisted_book_feed_item).collect())
}

pub(super) async fn load_latest_series_paged(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    library_id: Option<&str>,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, String> {
    load_latest_series_entries_paged(
        database_file,
        allowed_library_ids,
        library_id,
        offset,
        limit,
    )
    .await
    .map(|entries| entries.into_iter().map(persisted_series).collect())
}

pub(super) async fn load_library_series(
    database_file: &Path,
    library_id: &str,
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, String> {
    load_library_series_entries(database_file, library_id, offset, limit)
        .await
        .map(|entries| entries.into_iter().map(persisted_series).collect())
}

pub(super) async fn load_series_page(
    database_file: &Path,
    allowed_library_ids: &Option<HashSet<String>>,
    search: Option<&str>,
    publishers: &[String],
    offset: i64,
    limit: i64,
) -> Result<Vec<PersistedSeries>, String> {
    load_series_entries_page(
        database_file,
        allowed_library_ids,
        search,
        publishers,
        offset,
        limit,
    )
    .await
    .map(|entries| entries.into_iter().map(persisted_series).collect())
}

pub(super) async fn load_all_readlists(
    database_file: &Path,
) -> Result<Vec<PersistedReadlist>, String> {
    load_all_readlist_entries(database_file)
        .await
        .map(|entries| entries.into_iter().map(persisted_readlist).collect())
}
