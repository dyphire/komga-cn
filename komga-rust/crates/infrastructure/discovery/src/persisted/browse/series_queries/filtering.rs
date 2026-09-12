use std::collections::HashMap;

use komga_application::discovery::{
    BrowseContext, ScoredSearchHit, SeriesBrowseQuery, SeriesEvaluationContext,
    SeriesReadingDirection, SeriesRow, SeriesSortMode, collect_series_release_date_offsets,
    evaluate_series_condition, filter_and_paginate_series, series_condition_is_lightweight,
    series_condition_needs_collection_memberships, series_condition_needs_read_progress,
    series_condition_needs_total_book_counts,
};
use komga_domain::discovery::{
    InclusionCondition, PageEnvelope, SeriesCondition, SeriesStatus, SeriesValueCondition,
    system_locale_collator,
};

use super::super::models::{
    LightweightSeriesSortRow, PersistedSeriesBrowseQuery, PersistedSeriesSortMode,
    PersistedSeriesSummary,
};
use super::super::{DiscoveryQueryContext, SqliteDiscoveryBrowseService};
use super::super::sql_pushdown;

async fn enrich_series_read_progress(
    backend: &SqliteDiscoveryBrowseService,
    user_id: Option<&str>,
    mut content: Vec<PersistedSeriesSummary>,
) -> anyhow::Result<Vec<PersistedSeriesSummary>> {
    if let Some(user_id) = user_id {
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
    Ok(content)
}

/// RelevanceAsc mirrors the Kotlin contract: score desc + name asc + id
/// (higher relevance first). RelevanceDesc reverses both keys: score asc +
/// name desc + id.
fn sort_scored_hits_by_name_icu(
    mut hits: Vec<ScoredSearchHit>,
    names: &HashMap<String, String>,
    relevance_ascending: bool,
) -> Vec<ScoredSearchHit> {
    let collator = system_locale_collator();
    hits.sort_by(|left, right| {
        let score_ordering = if relevance_ascending {
            right.score.total_cmp(&left.score)
        } else {
            left.score.total_cmp(&right.score)
        };
        score_ordering
            .then_with(|| {
                let (left_name, right_name) = (
                    names.get(&left.id).map(String::as_str).unwrap_or_default(),
                    names
                        .get(&right.id)
                        .map(String::as_str)
                        .unwrap_or_default(),
                );
                if relevance_ascending {
                    collator.compare(left_name, right_name)
                } else {
                    collator.compare(right_name, left_name)
                }
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    hits
}

fn relevance_sort_is_ascending(sort_modes: &[PersistedSeriesSortMode]) -> bool {
    sort_modes.is_empty()
        || sort_modes
            .iter()
            .any(|mode| matches!(mode, PersistedSeriesSortMode::RelevanceAsc))
}

/// Multi-key sort for lightweight rows. Relevance keys expand to
/// (score, name, id) — exactly the rank ordering the slow path builds — and
/// read their score/name from the maps; remaining lightweight keys compare
/// on sort rows; full ties fall through to id.
/// Build the evaluation context for lightweight condition filtering on series.
/// Only the release-date cutoffs are needed (lightweight conditions never read
/// collections / read progress / total book counts).
async fn build_lightweight_series_eval_context(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    condition: Option<&SeriesCondition>,
) -> anyhow::Result<SeriesEvaluationContext> {
    let mut cutoffs = HashMap::new();
    if let Some(condition) = condition {
        for days in collect_series_release_date_offsets(condition) {
            cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
        }
    }
    Ok(SeriesEvaluationContext {
        user_id_present: context.user_id.is_some(),
        collection_memberships: None,
        read_progress: None,
        total_book_counts: None,
        read_dates: None,
        release_date_cutoffs: cutoffs,
    })
}

/// Build a SeriesRow view from a lightweight sort row. Only fields read by
/// lightweight condition evaluation and lightweight sort modes are populated.
fn lightweight_to_series_row(id: &str, row: &LightweightSeriesSortRow) -> SeriesRow {
    SeriesRow {
        id: id.to_string(),
        library_id: row.library_id.clone(),
        name: row.name.clone(),
        url: row.url.clone(),
        title: row.title.clone(),
        title_sort: row.title_sort.clone(),
        created: row.created_date.clone(),
        last_modified: row.last_modified_date.clone(),
        books_count: row.books_count,
        status: SeriesStatus::parse(&row.status).unwrap_or(SeriesStatus::Ongoing),
        publisher: row.publisher.clone(),
        age_rating: row.age_rating,
        language: row.language.clone(),
        books_metadata_release_date: row.release_date.clone(),
        deleted: row.deleted,
        oneshot: row.oneshot,
        ..SeriesRow::default()
    }
}

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

pub(in crate::persisted::browse) async fn load_persisted_series_page(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    query: PersistedSeriesBrowseQuery,
) -> anyhow::Result<PageEnvelope<PersistedSeriesSummary>> {
    let mut series = Vec::new();
    let mut relevance_ranks: HashMap<String, usize> = HashMap::new();

    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        // Non-Relevance searches unify on candidate-ID + SQL pushdown;
        // Relevance keeps the scored fast/slow paths (scores live in tantivy).
        let is_relevance_sort = query.sort_modes.is_empty()
            || query.sort_modes.iter().any(|mode| {
                matches!(
                    mode,
                    PersistedSeriesSortMode::RelevanceAsc | PersistedSeriesSortMode::RelevanceDesc
                )
            });
        if is_relevance_sort {
            let fast_path = query
                .condition
                .as_ref()
                .map_or(true, |condition| series_condition_is_lightweight(condition))
                && context.restrictions.is_none();
            if fast_path {
                // Empty sort modes on a search request mirror the slow path: hits
                // are ranked by sort_scored_hits_by_name_icu (score desc + name
                // asc + id) and the engine does not re-sort, i.e. RelevanceAsc.
                let total_count = backend.load_persisted_series_count().await?;
                let condition = query.condition.as_ref();
                let eval_ctx = build_lightweight_series_eval_context(backend, context, condition).await?;
                let scored_candidates = backend
                    .search_series_scored_ids(search, total_count.max(1))
                    .await?;
                let mut filtered_scored = scored_candidates;
                let candidate_ids: Vec<String> = filtered_scored
                    .iter()
                    .map(|hit| hit.id.clone())
                    .collect();
                let sort_rows = backend.load_series_sort_rows(&candidate_ids).await?;
                if let Some(condition) = condition {
                    filtered_scored.retain(|hit| {
                        sort_rows.get(&hit.id).is_some_and(|row| {
                            evaluate_series_condition(
                                &lightweight_to_series_row(&hit.id, row),
                                condition,
                                &eval_ctx,
                            )
                        })
                    });
                }
                let total_elements = filtered_scored.len();
                let candidate_ids: Vec<String> = filtered_scored
                    .iter()
                    .map(|hit| hit.id.clone())
                    .collect();
                let names = backend.load_series_names(&candidate_ids).await?;
                let ranked_ids: Vec<String> =
                    sort_scored_hits_by_name_icu(
                        filtered_scored,
                        &names,
                        relevance_sort_is_ascending(&query.sort_modes),
                    )
                    .into_iter()
                    .map(|hit| hit.id)
                    .collect();
                let page_ids: Vec<String> = if query.unpaged {
                    ranked_ids
                } else {
                    ranked_ids
                        .into_iter()
                        .skip(query.page.saturating_mul(query.size))
                        .take(query.size)
                        .collect()
                };
                let series = backend
                    .load_persisted_series_summaries_by_ids(&page_ids)
                    .await?;
                let content = enrich_series_read_progress(backend, context.user_id.as_deref(), series)
                    .await?;
                return Ok(if query.unpaged {
                    PageEnvelope::from_slice(content, 0, total_elements, total_elements)
                } else {
                    PageEnvelope::from_slice(content, query.page, query.size, total_elements)
                });
            }
            let total_count = backend.load_persisted_series_count().await?;
            let scored_candidates = backend
                .search_series_scored_ids(search, total_count.max(1))
                .await?;
            let candidate_ids: Vec<String> =
                scored_candidates.iter().map(|hit| hit.id.clone()).collect();
            if !candidate_ids.is_empty() {
                // Relevance half-mixed: SQL filters the candidate set down to
                // matching IDs (indexed EXISTS), then memory re-ranks by score
                // (score desc + name asc + id) and paginates in memory.
                if let Some(sql_page) = sql_pushdown::try_load_search_series_page(
                    backend,
                    context,
                    &query,
                    &candidate_ids,
                    false,
                )
                .await?
                {
                    let matched_ids = sql_page.ids;
                    let scores: HashMap<String, f32> = scored_candidates
                        .iter()
                        .map(|hit| (hit.id.clone(), hit.score))
                        .collect();
                    let names = backend.load_series_names(&matched_ids).await?;
                    let matched_hits: Vec<ScoredSearchHit> = matched_ids
                        .into_iter()
                        .map(|id| ScoredSearchHit {
                            score: scores.get(&id).copied().unwrap_or_default(),
                            id,
                        })
                        .collect();
                    let ranked_ids: Vec<String> =
                        sort_scored_hits_by_name_icu(
                            matched_hits,
                            &names,
                            relevance_sort_is_ascending(&query.sort_modes),
                        )
                        .into_iter()
                        .map(|hit| hit.id)
                        .collect();
                    let total_elements = ranked_ids.len();
                    let page_ids: Vec<String> = if query.unpaged {
                        ranked_ids
                    } else {
                        ranked_ids
                            .into_iter()
                            .skip(query.page.saturating_mul(query.size))
                            .take(query.size)
                            .collect()
                    };
                    let series = backend
                        .load_persisted_series_summaries_by_ids(&page_ids)
                        .await?;
                    let content = enrich_series_read_progress(
                        backend,
                        context.user_id.as_deref(),
                        series,
                    )
                    .await?;
                    return Ok(if query.unpaged {
                        PageEnvelope::from_slice(content, 0, total_elements, total_elements)
                    } else {
                        PageEnvelope::from_slice(content, query.page, query.size, total_elements)
                    });
                }
                // Fallback (unreachable in practice: translator only fails on
                // unsupported regex conditions): original slow path with full
                // candidate summaries evaluated in the engine.
                let names = backend.load_series_names(&candidate_ids).await?;
                let ranked_candidates = sort_scored_hits_by_name_icu(
                    scored_candidates,
                    &names,
                    relevance_sort_is_ascending(&query.sort_modes),
                );
                relevance_ranks = ranked_candidates
                    .iter()
                    .enumerate()
                    .map(|(index, hit)| (hit.id.clone(), index))
                    .collect();
                let ranked_ids: Vec<String> = ranked_candidates
                    .into_iter()
                    .map(|hit| hit.id)
                    .collect();
                series = backend
                    .load_persisted_series_summaries_by_ids(&ranked_ids)
                    .await?;
            }
        } else {
            // Candidate-ID + SQL mixed pushdown: tantivy narrows to candidate
            // IDs, SQL applies conditions/restrictions/order/pagination.
            let total_count = backend.load_persisted_series_count().await?;
            let all_ids: Vec<String> = backend
                .search_series_scored_ids(search, total_count.max(1))
                .await?
                .into_iter()
                .map(|hit| hit.id)
                .collect();
            if let Some(sql_page) = sql_pushdown::try_load_search_series_page(
                backend,
                context,
                &query,
                &all_ids,
                true,
            )
            .await?
            {
                let mut series = Vec::new();
                for chunk in sql_page.ids.chunks(500) {
                    series.extend(backend.load_persisted_series_summaries_by_ids(chunk).await?);
                }
                let content = enrich_series_read_progress(backend, context.user_id.as_deref(), series)
                    .await?;
                return Ok(PageEnvelope::from_slice(
                    content,
                    if query.unpaged { 0 } else { query.page },
                    if query.unpaged {
                        sql_page.total_elements
                    } else {
                        query.size
                    },
                    sql_page.total_elements,
                ));
            }
            // Fallback (unreachable in practice: the translator only returns
            // None for unsupported regex conditions): full-scan slow path.
            let scored_candidates = backend
                .search_series_scored_ids(search, total_count.max(1))
                .await?;
            let candidate_ids: Vec<String> =
                scored_candidates.iter().map(|hit| hit.id.clone()).collect();
            if !candidate_ids.is_empty() {
                let names = backend.load_series_names(&candidate_ids).await?;
                let ranked_candidates = sort_scored_hits_by_name_icu(
                    scored_candidates,
                    &names,
                    relevance_sort_is_ascending(&query.sort_modes),
                );
                relevance_ranks = ranked_candidates
                    .iter()
                    .enumerate()
                    .map(|(index, hit)| (hit.id.clone(), index))
                    .collect();
                let ranked_ids: Vec<String> = ranked_candidates
                    .into_iter()
                    .map(|hit| hit.id)
                    .collect();
                series = backend
                    .load_persisted_series_summaries_by_ids(&ranked_ids)
                    .await?;
            }
        }
    } else {

        if let Some(sql_page) = sql_pushdown::try_load_series_page(backend, context, &query).await? {
            let mut series = Vec::new();
            for chunk in sql_page.ids.chunks(500) {
                series.extend(backend.load_persisted_series_summaries_by_ids(chunk).await?);
            }
            let content = enrich_series_read_progress(backend, context.user_id.as_deref(), series)
                .await?;
            return Ok(PageEnvelope::from_slice(
                content,
                if query.unpaged { 0 } else { query.page },
                if query.unpaged {
                    sql_page.total_elements
                } else {
                    query.size
                },
                sql_page.total_elements,
            ));
        }
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
    let mut rows: Vec<SeriesRow> = series.into_iter().map(to_series_row).collect();
    // Empty sort semantics: stable ID order (mirrors SQL pushdown ORDER BY s.ID).
    if query.sort_modes.is_empty() {
        rows.sort_by(|a, b| a.id.cmp(&b.id));
    }

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
    let content = enrich_series_read_progress(
        backend,
        context.user_id.as_deref(),
        page.content
            .into_iter()
            .map(series_row_to_persisted)
            .collect(),
    )
    .await?;

    Ok(PageEnvelope::from_slice(
        content,
        page.page,
        page.page_size,
        page.total_elements,
    ))
}

async fn build_series_eval_context(
    backend: &SqliteDiscoveryBrowseService,
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
        url: row.url,
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
        status_lock: row.status_lock,
        summary: row.summary,
        summary_lock: row.summary_lock,
        reading_direction: SeriesReadingDirection::parse(&row.reading_direction),
        reading_direction_lock: row.reading_direction_lock,
        publisher: row.publisher,
        publisher_lock: row.publisher_lock,
        age_rating: row.age_rating,
        age_rating_lock: row.age_rating_lock,
        language: row.language,
        language_lock: row.language_lock,
        genres: row.genres,
        genres_lock: row.genres_lock,
        tags: row.tags,
        tags_lock: row.tags_lock,
        total_book_count: row.total_book_count,
        total_book_count_lock: row.total_book_count_lock,
        sharing_labels_lock: row.sharing_labels_lock,
        links: row.links,
        links_lock: row.links_lock,
        alternate_titles: row.alternate_titles,
        alternate_titles_lock: row.alternate_titles_lock,
        title_lock: row.title_lock,
        title_sort_lock: row.title_sort_lock,
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
        url: row.url,
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
        status_lock: row.status_lock,
        summary: row.summary,
        summary_lock: row.summary_lock,
        reading_direction: row
            .reading_direction
            .map(|value| value.persisted_name().to_string())
            .unwrap_or_default(),
        reading_direction_lock: row.reading_direction_lock,
        publisher: row.publisher,
        publisher_lock: row.publisher_lock,
        age_rating: row.age_rating,
        age_rating_lock: row.age_rating_lock,
        language: row.language,
        language_lock: row.language_lock,
        genres: row.genres,
        genres_lock: row.genres_lock,
        tags: row.tags,
        tags_lock: row.tags_lock,
        total_book_count: row.total_book_count,
        total_book_count_lock: row.total_book_count_lock,
        sharing_labels_lock: row.sharing_labels_lock,
        links: row.links,
        links_lock: row.links_lock,
        alternate_titles: row.alternate_titles,
        alternate_titles_lock: row.alternate_titles_lock,
        title_lock: row.title_lock,
        title_sort_lock: row.title_sort_lock,
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
