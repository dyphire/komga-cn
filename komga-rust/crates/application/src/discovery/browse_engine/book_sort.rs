use std::collections::HashMap;

use super::models::{BookRow, BookSortMode};

pub(super) fn sort_books(
    books: &mut [BookRow],
    sort_modes: &[BookSortMode],
    relevance_ranks: &HashMap<String, usize>,
    readlist_order: &HashMap<String, usize>,
) {
    if sort_modes.is_empty() {
        return;
    }

    books.sort_by(|left, right| {
        for sort_mode in sort_modes {
            let ordering = match sort_mode {
                BookSortMode::TitleAsc => left
                    .title
                    .to_ascii_lowercase()
                    .cmp(&right.title.to_ascii_lowercase()),
                BookSortMode::TitleDesc => right
                    .title
                    .to_ascii_lowercase()
                    .cmp(&left.title.to_ascii_lowercase()),
                BookSortMode::NameAsc => left
                    .name
                    .to_ascii_lowercase()
                    .cmp(&right.name.to_ascii_lowercase()),
                BookSortMode::NameDesc => right
                    .name
                    .to_ascii_lowercase()
                    .cmp(&left.name.to_ascii_lowercase()),
                BookSortMode::SeriesTitleAsc => left
                    .series_title_sort
                    .to_ascii_lowercase()
                    .cmp(&right.series_title_sort.to_ascii_lowercase()),
                BookSortMode::SeriesTitleDesc => right
                    .series_title_sort
                    .to_ascii_lowercase()
                    .cmp(&left.series_title_sort.to_ascii_lowercase()),
                BookSortMode::CreatedDateAsc => left.created.cmp(&right.created),
                BookSortMode::CreatedDateDesc => right.created.cmp(&left.created),
                BookSortMode::LastModifiedDateAsc => left.last_modified.cmp(&right.last_modified),
                BookSortMode::LastModifiedDateDesc => right.last_modified.cmp(&left.last_modified),
                BookSortMode::FileSizeAsc => left.size_bytes.cmp(&right.size_bytes),
                BookSortMode::FileSizeDesc => right.size_bytes.cmp(&left.size_bytes),
                BookSortMode::FileHashAsc => left.file_hash.cmp(&right.file_hash),
                BookSortMode::FileHashDesc => right.file_hash.cmp(&left.file_hash),
                BookSortMode::UrlAsc => left.url.cmp(&right.url),
                BookSortMode::UrlDesc => right.url.cmp(&left.url),
                BookSortMode::MediaStatusAsc => left
                    .media_status
                    .persisted_name()
                    .cmp(right.media_status.persisted_name()),
                BookSortMode::MediaStatusDesc => right
                    .media_status
                    .persisted_name()
                    .cmp(left.media_status.persisted_name()),
                BookSortMode::MediaCommentAsc => left.media_comment.cmp(&right.media_comment),
                BookSortMode::MediaCommentDesc => right.media_comment.cmp(&left.media_comment),
                BookSortMode::MediaTypeAsc => left.media_type.cmp(&right.media_type),
                BookSortMode::MediaTypeDesc => right.media_type.cmp(&left.media_type),
                BookSortMode::MediaPagesCountAsc => {
                    left.media_pages_count.cmp(&right.media_pages_count)
                }
                BookSortMode::MediaPagesCountDesc => {
                    right.media_pages_count.cmp(&left.media_pages_count)
                }
                BookSortMode::ReadProgressLastModifiedDateAsc => {
                    let left_date = left.read_progress.as_ref().map(|rp| &rp.last_modified);
                    let right_date = right.read_progress.as_ref().map(|rp| &rp.last_modified);
                    left_date.cmp(&right_date)
                }
                BookSortMode::ReadProgressLastModifiedDateDesc => {
                    let left_date = left.read_progress.as_ref().map(|rp| &rp.last_modified);
                    let right_date = right.read_progress.as_ref().map(|rp| &rp.last_modified);
                    right_date.cmp(&left_date)
                }
                BookSortMode::ReadProgressReadDateAsc => {
                    let left_date = left
                        .read_progress
                        .as_ref()
                        .and_then(|rp| rp.read_date.as_ref());
                    let right_date = right
                        .read_progress
                        .as_ref()
                        .and_then(|rp| rp.read_date.as_ref());
                    left_date.cmp(&right_date)
                }
                BookSortMode::ReadProgressReadDateDesc => {
                    let left_date = left
                        .read_progress
                        .as_ref()
                        .and_then(|rp| rp.read_date.as_ref());
                    let right_date = right
                        .read_progress
                        .as_ref()
                        .and_then(|rp| rp.read_date.as_ref());
                    right_date.cmp(&left_date)
                }
                BookSortMode::ReleaseDateAsc => {
                    left.metadata_release_date.cmp(&right.metadata_release_date)
                }
                BookSortMode::ReleaseDateDesc => {
                    right.metadata_release_date.cmp(&left.metadata_release_date)
                }
                BookSortMode::NumberSortAsc => left
                    .metadata_number_sort
                    .partial_cmp(&right.metadata_number_sort)
                    .unwrap_or(std::cmp::Ordering::Equal),
                BookSortMode::NumberSortDesc => right
                    .metadata_number_sort
                    .partial_cmp(&left.metadata_number_sort)
                    .unwrap_or(std::cmp::Ordering::Equal),
                BookSortMode::SeriesIdAsc => left.series_id.cmp(&right.series_id),
                BookSortMode::ReadListNumberAsc => readlist_order
                    .get(&left.id)
                    .cmp(&readlist_order.get(&right.id)),
                BookSortMode::ReadListNumberDesc => readlist_order
                    .get(&right.id)
                    .cmp(&readlist_order.get(&left.id)),
                BookSortMode::RelevanceAsc => {
                    compare_relevance_ranks(relevance_ranks, &left.id, &right.id, false)
                }
                BookSortMode::RelevanceDesc => {
                    compare_relevance_ranks(relevance_ranks, &left.id, &right.id, true)
                }
            };
            if ordering != std::cmp::Ordering::Equal {
                return ordering;
            }
        }
        left.id.cmp(&right.id)
    });
}

fn compare_relevance_ranks(
    ranks: &HashMap<String, usize>,
    left_id: &str,
    right_id: &str,
    descending: bool,
) -> std::cmp::Ordering {
    let left_rank = ranks.get(left_id).copied();
    let right_rank = ranks.get(right_id).copied();
    match (left_rank, right_rank) {
        (Some(left), Some(right)) if descending => {
            right.cmp(&left).then_with(|| left_id.cmp(right_id))
        }
        (Some(left), Some(right)) => left.cmp(&right).then_with(|| left_id.cmp(right_id)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => left_id.cmp(right_id),
    }
}
