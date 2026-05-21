pub mod book_condition;
pub mod book_sort;
pub mod helpers;
pub mod models;
pub mod series_condition;
pub mod series_sort;
pub mod text_matching;

use std::collections::BTreeSet;

use komga_domain::discovery::{BookCondition, BookValueCondition, SeriesCondition, SeriesValueCondition};

use models::*;

pub fn filter_and_paginate_books(
    mut rows: Vec<BookRow>,
    context: &BrowseContext,
    query: BookBrowseQuery,
    eval_ctx: BookEvaluationContext,
) -> Result<PageEnvelope<BookRow>, String> {
    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        rows.retain(|row| allowed_ids.iter().any(|id| id == &row.library_id));
    }

    if let Some(restrictions) = context.restrictions.as_ref() {
        if let (Some(age), Some(AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
        {
            rows.retain(|row| row.age_rating.map(|r| r < age).unwrap_or(true));
        }
    }

    if let Some(condition) = query.condition.as_ref() {
        rows.retain(|row| book_condition::evaluate(row, condition, &eval_ctx));
    }

    book_sort::sort_books(&mut rows, &query.sort_modes, &query.relevance_ranks, &query.readlist_order);

    paginate(rows, query.page, query.size, query.unpaged)
}

pub fn filter_and_paginate_series(
    mut rows: Vec<SeriesRow>,
    context: &BrowseContext,
    query: SeriesBrowseQuery,
    eval_ctx: SeriesEvaluationContext,
) -> Result<PageEnvelope<SeriesRow>, String> {
    if let Some(allowed_ids) = context.authorized_library_ids.as_ref() {
        rows.retain(|row| allowed_ids.iter().any(|id| id == &row.library_id));
    }

    if let Some(restrictions) = context.restrictions.as_ref() {
        if let (Some(age), Some(AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
        {
            rows.retain(|row| row.age_rating.map(|r| r < age).unwrap_or(true));
        }
        if !restrictions.labels_allow.is_empty() {
            let allowed = &restrictions.labels_allow;
            rows.retain(|row| {
                row.labels
                    .iter()
                    .any(|label| allowed.iter().any(|a| a.eq_ignore_ascii_case(label)))
            });
        }
        if !restrictions.labels_exclude.is_empty() {
            let excluded = &restrictions.labels_exclude;
            rows.retain(|row| {
                !row.labels
                    .iter()
                    .any(|label| excluded.iter().any(|e| e.eq_ignore_ascii_case(label)))
            });
        }
    }

    if let Some(condition) = query.condition.as_ref() {
        rows.retain(|row| series_condition::evaluate(row, condition, &eval_ctx));
    }

    series_sort::sort_series(
        &mut rows,
        &query.sort_modes,
        &query.relevance_ranks,
        &query.collection_order,
        &eval_ctx,
    );

    paginate(rows, query.page, query.size, query.unpaged)
}

pub fn book_condition_needs_readlist_memberships(condition: &BookCondition) -> bool {
    match condition {
        BookCondition::Value(BookValueCondition::ReadListId(_)) => true,
        BookCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(|c| book_condition_needs_readlist_memberships(c)),
        _ => false,
    }
}

pub fn book_condition_needs_posters(condition: &BookCondition) -> bool {
    match condition {
        BookCondition::Value(BookValueCondition::Poster(_)) => true,
        BookCondition::Composite(composite) => {
            composite.conditions.iter().any(|c| book_condition_needs_posters(c))
        }
        _ => false,
    }
}

pub fn collect_book_release_date_offsets(condition: &BookCondition) -> BTreeSet<i64> {
    let mut offsets = BTreeSet::new();
    collect_book_offsets_recursive(condition, &mut offsets);
    offsets
}

fn collect_book_offsets_recursive(condition: &BookCondition, offsets: &mut BTreeSet<i64>) {
    use komga_domain::discovery::DateCondition;
    match condition {
        BookCondition::Value(BookValueCondition::ReleaseDate(
            DateCondition::WithinLastDays(days) | DateCondition::OutsideLastDays(days),
        )) => {
            offsets.insert(*days);
        }
        BookCondition::Composite(composite) => {
            for child in &composite.conditions {
                collect_book_offsets_recursive(child, offsets);
            }
        }
        _ => {}
    }
}

pub fn series_condition_needs_collection_memberships(condition: &SeriesCondition) -> bool {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::CollectionId(_)) => true,
        SeriesCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(|c| series_condition_needs_collection_memberships(c)),
        _ => false,
    }
}

pub fn series_condition_needs_read_progress(condition: &SeriesCondition) -> bool {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::ReadStatus(_)) => true,
        SeriesCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(|c| series_condition_needs_read_progress(c)),
        _ => false,
    }
}

pub fn series_condition_needs_total_book_counts(condition: &SeriesCondition) -> bool {
    match condition {
        SeriesCondition::Value(SeriesValueCondition::Complete(_)) => true,
        SeriesCondition::Composite(composite) => composite
            .conditions
            .iter()
            .any(|c| series_condition_needs_total_book_counts(c)),
        _ => false,
    }
}

pub fn collect_series_release_date_offsets(condition: &SeriesCondition) -> BTreeSet<i64> {
    let mut offsets = BTreeSet::new();
    collect_series_offsets_recursive(condition, &mut offsets);
    offsets
}

fn collect_series_offsets_recursive(condition: &SeriesCondition, offsets: &mut BTreeSet<i64>) {
    use komga_domain::discovery::DateCondition;
    match condition {
        SeriesCondition::Value(SeriesValueCondition::ReleaseDate(
            DateCondition::WithinLastDays(days) | DateCondition::OutsideLastDays(days),
        )) => {
            offsets.insert(*days);
        }
        SeriesCondition::Composite(composite) => {
            for child in &composite.conditions {
                collect_series_offsets_recursive(child, offsets);
            }
        }
        _ => {}
    }
}

fn paginate<T>(rows: Vec<T>, page: usize, size: usize, unpaged: bool) -> Result<PageEnvelope<T>, String> {
    let total_elements = rows.len();
    if unpaged {
        return Ok(PageEnvelope {
            content: rows,
            page: 0,
            page_size: total_elements,
            total_elements,
        });
    }
    let start = page * size;
    let content = if start >= total_elements {
        vec![]
    } else {
        rows.into_iter().skip(start).take(size).collect()
    };
    Ok(PageEnvelope {
        content,
        page,
        page_size: size,
        total_elements,
    })
}
