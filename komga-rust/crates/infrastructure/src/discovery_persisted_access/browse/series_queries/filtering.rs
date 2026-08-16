use std::collections::HashMap;

use komga_application::discovery::{
    BrowseContext, SeriesBrowseQuery, SeriesEvaluationContext, SeriesReadingDirection, SeriesRow,
    SeriesSortMode, collect_series_release_date_offsets, filter_and_paginate_series,
    series_condition_needs_collection_memberships, series_condition_needs_read_progress,
    series_condition_needs_total_book_counts,
};
use komga_domain::discovery::{
    InclusionCondition, PageEnvelope, SeriesCondition, SeriesStatus, SeriesValueCondition,
};

use super::super::models::{
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode, PersistedSeriesSummary,
};
use super::super::{DiscoveryQueryContext, PersistedDiscoveryBrowseDataSource};

fn first_collection_sort_id(condition: Option<&SeriesCondition>) -> Option<&str> {
    fn visit(condition: &SeriesCondition) -> Option<&str> {
        match condition {
            SeriesCondition::Value(SeriesValueCondition::CollectionId(
                InclusionCondition::Include(values),
            )) => values.first().map(|value| value.as_str()),
            SeriesCondition::Composite(composite) => composite.conditions.iter().find_map(visit),
            _ => None,
        }
    }

    condition.and_then(visit)
}

pub(in crate::discovery_persisted_access::browse) async fn load_persisted_series_page(
    backend: &dyn PersistedDiscoveryBrowseDataSource,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> anyhow::Result<PageEnvelope<PersistedSeriesSummary>> {
    let mut series = Vec::new();
    let mut relevance_ranks: HashMap<String, usize> = HashMap::new();

    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = backend.load_persisted_series_count().await?;
        let ranked_candidates = backend
            .search_series_scored_ids(search, total_count.max(1))
            .await?;
        let candidate_ids: Vec<String> =
            ranked_candidates.iter().map(|hit| hit.id.clone()).collect();
        if !candidate_ids.is_empty() {
            relevance_ranks = ranked_candidates
                .iter()
                .enumerate()
                .map(|(index, hit)| (hit.id.clone(), index))
                .collect();
            series = backend
                .load_persisted_series_summaries_by_ids(&candidate_ids)
                .await?;
        }
    } else {
        series = backend.load_persisted_series_summaries().await?;
    }

    // Load collection ordering if needed for sort
    let collection_order = if query.sort_modes.iter().any(|m| {
        matches!(
            m,
            PersistedSeriesSortMode::CollectionNumberAsc
                | PersistedSeriesSortMode::CollectionNumberDesc
        )
    }) {
        if let Some(collection_id) = query
            .filters
            .collection_ids
            .as_ref()
            .and_then(|ids| ids.first().map(String::as_str))
            .or_else(|| first_collection_sort_id(query.condition.as_ref()))
        {
            backend
                .load_collection_ordering(collection_id)
                .await?
                .into_iter()
                .map(|(k, v)| (k, v as usize))
                .collect()
        } else {
            HashMap::new()
        }
    } else {
        HashMap::new()
    };

    // Load read dates if needed for sort
    let read_dates = if query.sort_modes.iter().any(|m| {
        matches!(
            m,
            PersistedSeriesSortMode::ReadDateAsc | PersistedSeriesSortMode::ReadDateDesc
        )
    }) {
        if let Some(user_id) = context.user_id.as_deref() {
            Some(backend.load_series_read_dates(user_id).await?)
        } else {
            None
        }
    } else {
        None
    };

    // Build evaluation context
    let eval_ctx =
        build_series_eval_context(backend, context, query.condition.as_ref(), read_dates).await?;

    // Map to engine types
    let rows: Vec<SeriesRow> = series.into_iter().map(to_series_row).collect();

    let browse_ctx = to_browse_context(context);
    let engine_query = SeriesBrowseQuery {
        condition: query.condition,
        page: query.page,
        size: query.size,
        unpaged: query.unpaged,
        sort_modes: query
            .sort_modes
            .iter()
            .filter_map(to_series_sort_mode)
            .collect(),
        relevance_ranks,
        collection_order,
    };

    let page = filter_and_paginate_series(rows, &browse_ctx, engine_query, eval_ctx)?;

    // Enrich read progress counts on the paginated result
    let mut content: Vec<PersistedSeriesSummary> = page
        .content
        .into_iter()
        .map(series_row_to_persisted)
        .collect();

    if let Some(user_id) = context.user_id.as_deref() {
        let read_progress = backend.load_series_read_progress_counts(user_id).await?;
        for row in &mut content {
            let counts = read_progress.get(&row.id).copied().unwrap_or_default();
            row.books_read_count = counts.read_count.max(0) as u64;
            row.books_in_progress_count = counts.in_progress_count.max(0) as u64;
            row.books_unread_count = row
                .books_count
                .saturating_sub(row.books_read_count + row.books_in_progress_count);
        }
    }

    Ok(PageEnvelope::from_slice(
        content,
        page.page,
        page.page_size,
        page.total_elements,
    ))
}

async fn build_series_eval_context(
    backend: &dyn PersistedDiscoveryBrowseDataSource,
    context: &DiscoveryQueryContext,
    condition: Option<&SeriesCondition>,
    read_dates: Option<HashMap<String, String>>,
) -> anyhow::Result<SeriesEvaluationContext> {
    let mut eval_context = SeriesEvaluationContext {
        user_id_present: context.user_id.is_some(),
        collection_memberships: None,
        read_progress: None,
        total_book_counts: None,
        read_dates,
        release_date_cutoffs: HashMap::new(),
    };

    let Some(condition) = condition else {
        return Ok(eval_context);
    };

    if series_condition_needs_collection_memberships(condition) {
        eval_context.collection_memberships = Some(backend.load_collection_memberships().await?);
    }

    if series_condition_needs_read_progress(condition)
        && let Some(user_id) = context.user_id.as_deref()
    {
        eval_context.read_progress = Some(backend.load_series_read_progress_counts(user_id).await?);
    }

    if series_condition_needs_total_book_counts(condition) {
        eval_context.total_book_counts = Some(backend.load_series_total_book_counts().await?);
    }

    for days in collect_series_release_date_offsets(condition) {
        eval_context
            .release_date_cutoffs
            .insert(days, backend.persisted_utc_date_minus_days(days).await?);
    }

    Ok(eval_context)
}

fn to_browse_context(context: &DiscoveryQueryContext) -> BrowseContext {
    BrowseContext {
        user_id: context.user_id.clone(),
        is_admin: context.is_admin,
        authorized_library_ids: context.authorized_library_ids.clone(),
        restrictions: context.restrictions.clone(),
    }
}

fn to_series_row(row: PersistedSeriesSummary) -> SeriesRow {
    SeriesRow {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
        title_sort: row.title_sort,
        labels: row.labels,
        created: row.created,
        last_modified: row.last_modified,
        file_last_modified: row.file_last_modified,
        books_count: row.books_count,
        books_read_count: row.books_read_count,
        books_unread_count: row.books_unread_count,
        books_in_progress_count: row.books_in_progress_count,
        status: SeriesStatus::parse(&row.status).unwrap_or(SeriesStatus::Ongoing),
        summary: row.summary,
        reading_direction: SeriesReadingDirection::parse(&row.reading_direction),
        publisher: row.publisher,
        age_rating: row.age_rating,
        language: row.language,
        genres: row.genres,
        tags: row.tags,
        alternate_titles: row.alternate_titles,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        books_metadata_authors: row.books_metadata_authors,
        books_metadata_tags: row.books_metadata_tags,
        books_metadata_release_date: row.books_metadata_release_date,
        books_metadata_summary: row.books_metadata_summary,
        books_metadata_summary_number: row.books_metadata_summary_number,
        books_metadata_created: row.books_metadata_created,
        books_metadata_last_modified: row.books_metadata_last_modified,
        deleted: row.deleted,
        oneshot: row.oneshot,
    }
}

fn series_row_to_persisted(row: SeriesRow) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: row.id,
        library_id: row.library_id,
        name: row.name,
        title: row.title,
        title_sort: row.title_sort,
        labels: row.labels,
        created: row.created,
        last_modified: row.last_modified,
        file_last_modified: row.file_last_modified,
        books_count: row.books_count,
        books_read_count: row.books_read_count,
        books_unread_count: row.books_unread_count,
        books_in_progress_count: row.books_in_progress_count,
        status: row.status.persisted_name().to_string(),
        summary: row.summary,
        reading_direction: row
            .reading_direction
            .map(|value| value.persisted_name().to_string())
            .unwrap_or_default(),
        publisher: row.publisher,
        age_rating: row.age_rating,
        language: row.language,
        genres: row.genres,
        tags: row.tags,
        alternate_titles: row.alternate_titles,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        books_metadata_authors: row.books_metadata_authors,
        books_metadata_tags: row.books_metadata_tags,
        books_metadata_release_date: row.books_metadata_release_date,
        books_metadata_summary: row.books_metadata_summary,
        books_metadata_summary_number: row.books_metadata_summary_number,
        books_metadata_created: row.books_metadata_created,
        books_metadata_last_modified: row.books_metadata_last_modified,
        deleted: row.deleted,
        oneshot: row.oneshot,
    }
}

fn to_series_sort_mode(mode: &PersistedSeriesSortMode) -> Option<SeriesSortMode> {
    Some(match mode {
        PersistedSeriesSortMode::TitleAsc => SeriesSortMode::TitleAsc,
        PersistedSeriesSortMode::TitleDesc => SeriesSortMode::TitleDesc,
        PersistedSeriesSortMode::NameAsc => SeriesSortMode::NameAsc,
        PersistedSeriesSortMode::NameDesc => SeriesSortMode::NameDesc,
        PersistedSeriesSortMode::ReadDateAsc => SeriesSortMode::ReadDateAsc,
        PersistedSeriesSortMode::ReadDateDesc => SeriesSortMode::ReadDateDesc,
        PersistedSeriesSortMode::CollectionNumberAsc => SeriesSortMode::CollectionNumberAsc,
        PersistedSeriesSortMode::CollectionNumberDesc => SeriesSortMode::CollectionNumberDesc,
        PersistedSeriesSortMode::Random => SeriesSortMode::Random,
        PersistedSeriesSortMode::CreatedAsc => SeriesSortMode::CreatedAsc,
        PersistedSeriesSortMode::CreatedDesc => SeriesSortMode::CreatedDesc,
        PersistedSeriesSortMode::LastModifiedAsc => SeriesSortMode::LastModifiedAsc,
        PersistedSeriesSortMode::LastModifiedDesc => SeriesSortMode::LastModifiedDesc,
        PersistedSeriesSortMode::ReleaseDateAsc => SeriesSortMode::ReleaseDateAsc,
        PersistedSeriesSortMode::ReleaseDateDesc => SeriesSortMode::ReleaseDateDesc,
        PersistedSeriesSortMode::BooksCountAsc => SeriesSortMode::BooksCountAsc,
        PersistedSeriesSortMode::BooksCountDesc => SeriesSortMode::BooksCountDesc,
        PersistedSeriesSortMode::RelevanceAsc => SeriesSortMode::RelevanceAsc,
        PersistedSeriesSortMode::RelevanceDesc => SeriesSortMode::RelevanceDesc,
    })
}
