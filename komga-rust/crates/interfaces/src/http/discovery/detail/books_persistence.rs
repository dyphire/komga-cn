use super::*;

use crate::discovery_detail_access::books as books_access;

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
    database_file: &FsPath,
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
        load_persisted_book_resource(database_file, requested_book_id).await,
        Ok(Some(_))
    ) {
        return requested_book_id.to_string();
    }

    match books_access::load_book_id_by_sorted_position(database_file, index).await {
        Ok(Some(book_id)) => book_id,
        _ => requested_book_id.to_string(),
    }
}

pub async fn load_persisted_book_resource(
    database_file: &FsPath,
    book_id: &str,
) -> Result<Option<PersistedBookResource>, String> {
    let resource = books_access::load_persisted_book_resource(database_file, book_id)
        .await?
        .map(|row| PersistedBookResource {
            library_id: row.library_id,
            age_rating: row.age_rating,
            sharing_labels: parse_csv_values(&row.sharing_labels),
        });
    Ok(resource)
}

pub async fn load_persisted_book_detail(
    database_file: &FsPath,
    book_id: &str,
    user_id: Option<&str>,
) -> Result<Option<BookDetailReadModel>, String> {
    let model = books_access::load_persisted_book_detail(database_file, book_id, user_id)
        .await?
        .map(|row| BookDetailReadModel {
            id: row.id,
            series_id: row.series_id,
            series_title: row.series_title,
            library_id: row.library_id,
            name: row.name,
            url: row.url,
            number: row.number,
            created: row.created,
            last_modified: row.last_modified,
            file_last_modified: row.file_last_modified,
            size_bytes: row.size_bytes,
            media_status: row.media_status,
            media_type: row.media_type,
            media_pages_count: row.media_pages_count,
            media_comment: row.media_comment,
            metadata_title: row.metadata_title,
            metadata_summary: row.metadata_summary,
            metadata_number: row.metadata_number,
            metadata_number_sort: row.metadata_number_sort,
            metadata_release_date: row.metadata_release_date,
            metadata_authors: parse_csv_values(&row.metadata_authors),
            metadata_tags: parse_csv_values(&row.metadata_tags),
            metadata_isbn: row.metadata_isbn,
            metadata_created: row.metadata_created,
            metadata_last_modified: row.metadata_last_modified,
            read_progress: row.read_progress.map(|progress| PersistedReadProgress {
                page: progress.page,
                completed: progress.completed,
                read_date: progress.read_date,
                created: progress.created,
                last_modified: progress.last_modified,
                device_id: progress.device_id,
                device_name: progress.device_name,
            }),
            deleted: row.deleted,
            file_hash: row.file_hash,
            oneshot: row.oneshot,
        });
    Ok(model)
}

pub async fn load_persisted_book_sibling_detail(
    database_file: &FsPath,
    book_id: &str,
    direction: PersistedBookSiblingDirection,
    user_id: Option<&str>,
) -> Result<Option<BookDetailReadModel>, String> {
    let direction = match direction {
        PersistedBookSiblingDirection::Previous => {
            books_access::PersistedBookSiblingDirectionRecord::Previous
        }
        PersistedBookSiblingDirection::Next => {
            books_access::PersistedBookSiblingDirectionRecord::Next
        }
    };

    let Some(sibling_id) =
        books_access::load_persisted_book_sibling_id(database_file, book_id, direction).await?
    else {
        return Ok(None);
    };

    load_persisted_book_detail(database_file, &sibling_id, user_id).await
}
