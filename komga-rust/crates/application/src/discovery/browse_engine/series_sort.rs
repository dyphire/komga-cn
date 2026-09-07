use std::collections::HashMap;

use komga_domain::discovery::compare_book_names;

use super::models::{SeriesEvaluationContext, SeriesRow, SeriesSortMode};

pub(super) fn sort_series(
    series: &mut [SeriesRow],
    sort_modes: &[SeriesSortMode],
    relevance_ranks: &HashMap<String, usize>,
    collection_order: &HashMap<String, usize>,
    eval_ctx: &SeriesEvaluationContext,
) {
    if sort_modes.is_empty() {
        return;
    }

    let random_keys = if sort_modes
        .iter()
        .any(|m| matches!(m, SeriesSortMode::Random))
    {
        random_sort_keys(series)
    } else {
        HashMap::new()
    };

    series.sort_by(|left, right| {
        for sort_mode in sort_modes {
            let ordering = match sort_mode {
                SeriesSortMode::TitleAsc => compare_book_names(&left.title_sort, &right.title_sort),
                SeriesSortMode::TitleDesc => compare_book_names(&right.title_sort, &left.title_sort),
                SeriesSortMode::NameAsc => compare_book_names(&left.name, &right.name),
                SeriesSortMode::NameDesc => compare_book_names(&right.name, &left.name),
                SeriesSortMode::ReadDateAsc => {
                    let left_date = eval_ctx.read_dates.as_ref().and_then(|d| d.get(&left.id));
                    let right_date = eval_ctx.read_dates.as_ref().and_then(|d| d.get(&right.id));
                    left_date.cmp(&right_date)
                }
                SeriesSortMode::ReadDateDesc => {
                    let left_date = eval_ctx.read_dates.as_ref().and_then(|d| d.get(&left.id));
                    let right_date = eval_ctx.read_dates.as_ref().and_then(|d| d.get(&right.id));
                    right_date.cmp(&left_date)
                }
                SeriesSortMode::CollectionNumberAsc => collection_order
                    .get(&left.id)
                    .cmp(&collection_order.get(&right.id)),
                SeriesSortMode::CollectionNumberDesc => collection_order
                    .get(&right.id)
                    .cmp(&collection_order.get(&left.id)),
                SeriesSortMode::Random => {
                    random_keys.get(&left.id).cmp(&random_keys.get(&right.id))
                }
                SeriesSortMode::CreatedAsc => left.created.cmp(&right.created),
                SeriesSortMode::CreatedDesc => right.created.cmp(&left.created),
                SeriesSortMode::LastModifiedAsc => left.last_modified.cmp(&right.last_modified),
                SeriesSortMode::LastModifiedDesc => right.last_modified.cmp(&left.last_modified),
                SeriesSortMode::ReleaseDateAsc => left
                    .books_metadata_release_date
                    .cmp(&right.books_metadata_release_date),
                SeriesSortMode::ReleaseDateDesc => right
                    .books_metadata_release_date
                    .cmp(&left.books_metadata_release_date),
                SeriesSortMode::BooksCountAsc => left.books_count.cmp(&right.books_count),
                SeriesSortMode::BooksCountDesc => right.books_count.cmp(&left.books_count),
                SeriesSortMode::RelevanceAsc => {
                    compare_relevance_ranks(relevance_ranks, &left.id, &right.id, false)
                }
                SeriesSortMode::RelevanceDesc => {
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

fn random_sort_keys(series: &[SeriesRow]) -> HashMap<String, u64> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    series
        .iter()
        .map(|row| {
            let mut hasher = DefaultHasher::new();
            row.id.hash(&mut hasher);
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .hash(&mut hasher);
            (row.id.clone(), hasher.finish())
        })
        .collect()
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
