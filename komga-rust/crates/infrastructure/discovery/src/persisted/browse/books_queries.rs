use std::collections::{HashMap, HashSet};

use komga_application::discovery::{
    AuthorEntry, BookBrowseQuery, BookEvaluationContext, BookMetadataAuthorReadModel,
    BookMetadataLinkReadModel, BookPosterRow, BookReadProgressReadModel, BookRow, BookSortMode,
    BrowseContext, ReadProgressRow, ScoredSearchHit, WebLinkEntry, book_condition_is_lightweight,
    book_condition_needs_posters, book_condition_needs_readlist_memberships,
    collect_book_release_date_offsets, evaluate_book_condition, filter_and_paginate_books,
};
use komga_domain::discovery::{
    BookCondition, BookValueCondition, InclusionCondition, MediaStatus, PageEnvelope,
    system_locale_collator,
};

use super::models::{
    LightweightBookSortRow, PersistedBookSummary, PersistedBooksBrowseQuery,
    PersistedBooksSortMode,
};
use super::{DiscoveryQueryContext, SqliteDiscoveryBrowseService};
use super::sql_pushdown;

use komga_application::discovery::BookReadModel;

fn intersect_library_ids(
    requested: Option<&[String]>,
    authorized: Option<&[String]>,
) -> Option<Vec<String>> {
    match (requested, authorized) {
        (None, None) => None,
        (Some(requested), None) => Some(requested.to_vec()),
        (None, Some(authorized)) => Some(authorized.to_vec()),
        (Some(requested), Some(authorized)) => {
            let authorized_set: HashSet<&str> = authorized.iter().map(String::as_str).collect();
            Some(
                requested
                    .iter()
                    .filter(|id| authorized_set.contains(id.as_str()))
                    .cloned()
                    .collect(),
            )
        }
    }
}

/// RelevanceAsc mirrors the Kotlin contract: score desc + title asc + id
/// (higher relevance first). RelevanceDesc reverses both keys: score asc +
/// title desc + id.
fn sort_scored_hits_by_title_icu(
    mut hits: Vec<ScoredSearchHit>,
    titles: &HashMap<String, String>,
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
                let (left_title, right_title) = (
                    titles.get(&left.id).map(String::as_str).unwrap_or_default(),
                    titles
                        .get(&right.id)
                        .map(String::as_str)
                        .unwrap_or_default(),
                );
                if relevance_ascending {
                    collator.compare(left_title, right_title)
                } else {
                    collator.compare(right_title, left_title)
                }
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    hits
}

fn relevance_sort_is_ascending(sort_modes: &[PersistedBooksSortMode]) -> bool {
    sort_modes.is_empty()
        || sort_modes
            .iter()
            .any(|mode| matches!(mode, PersistedBooksSortMode::RelevanceAsc))
}

/// Build the evaluation context for lightweight condition filtering. Only the
/// release-date cutoffs are needed (A/B-class conditions never read
/// readlist/posters/read-progress), so those stay empty.
async fn build_lightweight_eval_context(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    condition: Option<&BookCondition>,
) -> anyhow::Result<BookEvaluationContext> {
    let mut cutoffs = HashMap::new();
    if let Some(condition) = condition {
        for days in collect_book_release_date_offsets(condition) {
            cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
        }
    }
    Ok(BookEvaluationContext {
        user_id_present: context.user_id.is_some(),
        readlist_memberships: None,
        posters: None,
        release_date_cutoffs: cutoffs,
    })
}

/// Build a BookRow view from a lightweight sort row. Only fields read by
/// A/B-class condition evaluation are populated; the rest default so that
/// lightweight conditions evaluate exactly like the in-memory engine.
fn lightweight_to_book_row(id: &str, row: &LightweightBookSortRow) -> BookRow {
    BookRow {
        id: id.to_string(),
        series_id: row.series_id.clone(),
        library_id: row.library_id.clone(),
        series_title_sort: row.series_title_sort.clone(),
        title: row.title.clone(),
        name: row.name.clone(),
        url: row.url.clone(),
        created: row.created_date.clone(),
        last_modified: row.last_modified_date.clone(),
        media_status: MediaStatus::parse(&row.media_status).unwrap_or(MediaStatus::Unknown),
        media_type: row.media_type.clone(),
        media_pages_count: row.media_pages_count as u32,
        media_comment: row.media_comment.clone(),
        metadata_number_sort: row.number_sort,
        metadata_release_date: row.release_date.clone(),
        file_hash: row.file_hash.clone(),
        deleted: row.deleted,
        oneshot: row.oneshot,
        language: row.language.clone(),
        publisher: row.publisher.clone(),
        age_rating: row.age_rating,
        ..BookRow::default()
    }
}

/// Multi-key sort for lightweight rows. Relevance keys expand to
/// (score, title, id) — exactly the rank ordering the slow path builds — and
/// read their score/title from the maps; remaining lightweight keys compare
/// on sort rows; full ties fall through to the engine tie-break.
fn first_readlist_sort_id(condition: Option<&BookCondition>) -> Option<&str> {
    fn visit(condition: &BookCondition) -> Option<&str> {
        match condition {
            BookCondition::Value(BookValueCondition::ReadListId(InclusionCondition::Include(
                values,
            ))) => values.first().map(|value| value.as_str()),
            BookCondition::Composite(composite) => composite.conditions.iter().find_map(visit),
            _ => None,
        }
    }

    condition.and_then(visit)
}

pub(super) async fn load_persisted_books_page(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> anyhow::Result<PageEnvelope<BookReadModel>> {
    let mut books = Vec::new();
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
                    PersistedBooksSortMode::RelevanceAsc | PersistedBooksSortMode::RelevanceDesc
                )
            });
        if is_relevance_sort {
            // Any sort keys beyond Relevance are shadowed by the unique
            let fast_path = query
                .condition
                .as_ref()
                .map_or(true, |condition| book_condition_is_lightweight(condition))
                && context.restrictions.is_none();
            if fast_path {
                // Empty sort modes on a search request mirror the slow path: hits
                // are ranked by sort_scored_hits_by_title_icu (score desc + title
                // asc + id) and the engine does not re-sort, i.e. RelevanceAsc.
                let filter_library_ids = intersect_library_ids(
                    query
                        .filters
                        .library_ids
                        .as_deref()
                        .filter(|ids| !ids.is_empty()),
                    context
                        .authorized_library_ids
                        .as_deref()
                        .filter(|ids| !ids.is_empty()),
                );
                let total_count = backend.load_persisted_book_count().await?;
                let condition = query.condition.as_ref();
                let eval_ctx = build_lightweight_eval_context(backend, context, condition).await?;
                let scored_candidates = backend
                    .search_book_scored_ids(search, total_count.max(1))
                    .await?;
                let mut filtered_scored = match &filter_library_ids {
                    Some(library_ids) if library_ids.is_empty() => Vec::new(),
                    Some(library_ids) => {
                        backend
                            .filter_scored_book_ids_by_library(&scored_candidates, library_ids)
                            .await?
                    }
                    None => scored_candidates,
                };
                let candidate_ids: Vec<String> = filtered_scored
                    .iter()
                    .map(|hit| hit.id.clone())
                    .collect();
                let sort_rows = backend.load_book_sort_rows(&candidate_ids).await?;
                if let Some(condition) = condition {
                    filtered_scored.retain(|hit| {
                        sort_rows.get(&hit.id).is_some_and(|row| {
                            evaluate_book_condition(
                                &lightweight_to_book_row(&hit.id, row),
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
                let titles = backend.load_book_titles(&candidate_ids).await?;
                let ranked_ids: Vec<String> =
                    sort_scored_hits_by_title_icu(
                        filtered_scored,
                        &titles,
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
                let books = backend
                    .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), &page_ids)
                    .await?;
                let content: Vec<BookReadModel> = books
                    .into_iter()
                    .map(to_book_row)
                    .map(book_row_to_read_model)
                    .collect();
                return Ok(if query.unpaged {
                    PageEnvelope::from_slice(content, 0, total_elements, total_elements)
                } else {
                    PageEnvelope::from_slice(content, query.page, query.size, total_elements)
                });
            }
            let total_count = backend.load_persisted_book_count().await?;
            let scored_candidates = backend
                .search_book_scored_ids(search, total_count.max(1))
                .await?;
            let candidate_ids: Vec<String> = scored_candidates
                .iter()
                .map(|hit| hit.id.clone())
                .collect();
            if !candidate_ids.is_empty() {
                // Relevance half-mixed: SQL filters the candidate set down to
                // matching IDs (indexed EXISTS), then memory re-ranks by score
                // (score desc + title asc + id) and paginates in memory.
                if let Some(sql_page) = sql_pushdown::try_load_search_books_page(
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
                    let titles = backend.load_book_titles(&matched_ids).await?;
                    let matched_hits: Vec<ScoredSearchHit> = matched_ids
                        .into_iter()
                        .map(|id| ScoredSearchHit {
                            score: scores.get(&id).copied().unwrap_or_default(),
                            id,
                        })
                        .collect();
                    let ranked_ids: Vec<String> =
                        sort_scored_hits_by_title_icu(
                            matched_hits,
                            &titles,
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
                    let books = backend
                        .load_persisted_book_summaries_by_ids(
                            context.user_id.as_deref(),
                            &page_ids,
                        )
                        .await?;
                    let content: Vec<BookReadModel> = books
                        .into_iter()
                        .map(to_book_row)
                        .map(book_row_to_read_model)
                        .collect();
                    return Ok(if query.unpaged {
                        PageEnvelope::from_slice(content, 0, total_elements, total_elements)
                    } else {
                        PageEnvelope::from_slice(content, query.page, query.size, total_elements)
                    });
                }
                // Fallback (unreachable in practice: translator only fails on
                // unsupported regex conditions): original slow path with full
                // candidate summaries evaluated in the engine.
                let titles = backend.load_book_titles(&candidate_ids).await?;
                let ranked_candidates = sort_scored_hits_by_title_icu(
                    scored_candidates,
                    &titles,
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
                books = backend
                    .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), &ranked_ids)
                    .await?;
            }
        } else {
            // Candidate-ID + SQL mixed pushdown: tantivy narrows to candidate
            // IDs, SQL applies conditions/restrictions/order/pagination.
            let total_count = backend.load_persisted_book_count().await?;
            let all_ids = backend.search_book_ids(search, total_count.max(1)).await?;
            if let Some(sql_page) = sql_pushdown::try_load_search_books_page(
                backend,
                context,
                &query,
                &all_ids,
                true,
            )
            .await?
            {
                let mut books = Vec::new();
                for chunk in sql_page.ids.chunks(500) {
                    books.extend(
                        backend
                            .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), chunk)
                            .await?,
                    );
                }
                let content: Vec<BookReadModel> = books
                    .into_iter()
                    .map(to_book_row)
                    .map(book_row_to_read_model)
                    .collect();
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
                .search_book_scored_ids(search, total_count.max(1))
                .await?;
            let candidate_ids: Vec<String> = scored_candidates
                .iter()
                .map(|hit| hit.id.clone())
                .collect();
            if !candidate_ids.is_empty() {
                let titles = backend.load_book_titles(&candidate_ids).await?;
                let ranked_candidates = sort_scored_hits_by_title_icu(
                    scored_candidates,
                    &titles,
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
                books = backend
                    .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), &ranked_ids)
                    .await?;
            }
        }
    } else {

        if let Some(sql_page) = sql_pushdown::try_load_books_page(backend, context, &query).await? {
            let mut books = Vec::new();
            for chunk in sql_page.ids.chunks(500) {
                books.extend(
                    backend
                        .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), chunk)
                        .await?,
                );
            }
            let content: Vec<BookReadModel> = books
                .into_iter()
                .map(to_book_row)
                .map(book_row_to_read_model)
                .collect();
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
        books = backend
            .load_persisted_book_summaries(context.user_id.as_deref())
            .await?;
    }

    // Handle library_ids from flat filter (only used by list_latest_books)
    if let Some(library_ids) = query.filters.library_ids.as_ref() {
        books.retain(|row| library_ids.iter().any(|id| id == row.library_id.as_str()));
    }

    // Load readlist ordering if needed for sort
    let readlist_order = if query.sort_modes.iter().any(|m| {
        matches!(
            m,
            PersistedBooksSortMode::ReadListNumberAsc | PersistedBooksSortMode::ReadListNumberDesc
        )
    }) {
        if let Some(readlist_id) = first_readlist_sort_id(query.condition.as_ref()) {
            backend
                .load_readlist_ordering(readlist_id)
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

    // Build evaluation context
    let eval_ctx = build_book_eval_context(backend, context, query.condition.as_ref()).await?;

    // Map to engine types
    let mut rows: Vec<BookRow> = books.into_iter().map(to_book_row).collect();
    // Empty sort semantics: stable ID order (mirrors SQL pushdown ORDER BY b.ID).
    if query.sort_modes.is_empty() {
        rows.sort_by(|a, b| a.id.cmp(&b.id));
    }

    let browse_ctx = to_browse_context(context);
    let engine_query = BookBrowseQuery {
        condition: query.condition,
        page: query.page,
        size: query.size,
        unpaged: query.unpaged,
        sort_modes: query
            .sort_modes
            .iter()
            .filter_map(to_book_sort_mode)
            .collect(),
        relevance_ranks,
        readlist_order,
    };

    let page = filter_and_paginate_books(rows, &browse_ctx, engine_query, eval_ctx)?;

    Ok(PageEnvelope::from_slice(
        page.content
            .into_iter()
            .map(book_row_to_read_model)
            .collect(),
        page.page,
        page.page_size,
        page.total_elements,
    ))
}

async fn build_book_eval_context(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    condition: Option<&BookCondition>,
) -> anyhow::Result<BookEvaluationContext> {
    let mut eval_context = BookEvaluationContext {
        user_id_present: context.user_id.is_some(),
        readlist_memberships: None,
        posters: None,
        release_date_cutoffs: HashMap::new(),
    };

    let Some(condition) = condition else {
        return Ok(eval_context);
    };

    if book_condition_needs_readlist_memberships(condition) {
        eval_context.readlist_memberships = Some(backend.load_readlist_memberships().await?);
    }

    if book_condition_needs_posters(condition) {
        eval_context.posters = Some(
            backend
                .load_book_poster_summaries()
                .await?
                .into_iter()
                .map(|(id, posters)| {
                    (
                        id,
                        posters
                            .into_iter()
                            .map(|p| BookPosterRow {
                                thumbnail_type: p.thumbnail_type,
                                selected: p.selected,
                            })
                            .collect(),
                    )
                })
                .collect(),
        );
    }

    for days in collect_book_release_date_offsets(condition) {
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

fn to_book_row(row: PersistedBookSummary) -> BookRow {
    BookRow {
        id: row.id,
        series_id: row.series_id,
        library_id: row.library_id,
        series_title: row.series_title,
        series_title_sort: row.series_title_sort,
        title: row.title,
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
        media_epub_divina_compatible: row.media_epub_divina_compatible,
        media_epub_is_kepub: row.media_epub_is_kepub,
        read_status: row.read_status,
        metadata_title_lock: row.metadata_title_lock,
        metadata_summary: row.metadata_summary,
        metadata_summary_lock: row.metadata_summary_lock,
        metadata_number: row.metadata_number,
        metadata_number_lock: row.metadata_number_lock,
        metadata_number_sort: row.metadata_number_sort,
        metadata_number_sort_lock: row.metadata_number_sort_lock,
        metadata_release_date: row.metadata_release_date,
        metadata_release_date_lock: row.metadata_release_date_lock,
        metadata_authors_lock: row.metadata_authors_lock,
        metadata_tags_lock: row.metadata_tags_lock,
        metadata_isbn: row.metadata_isbn,
        metadata_isbn_lock: row.metadata_isbn_lock,
        metadata_links_lock: row.metadata_links_lock,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        file_hash: row.file_hash,
        read_progress: row.read_progress.map(|p| ReadProgressRow {
            page: p.page,
            completed: p.completed,
            read_date: p.read_date,
            created: p.created,
            last_modified: p.last_modified,
            device_id: p.device_id,
            device_name: p.device_name,
        }),
        deleted: row.deleted,
        oneshot: row.oneshot,
        genres: row.genres,
        language: row.language,
        publisher: row.publisher,
        age_rating: row.age_rating,
        metadata_tags: row.metadata_tags,
        metadata_authors: row
            .metadata_authors
            .into_iter()
            .map(|a| AuthorEntry {
                name: a.name,
                role: a.role,
            })
            .collect(),
        metadata_links: row
            .metadata_links
            .into_iter()
            .map(|l| WebLinkEntry {
                label: l.label,
                url: l.url,
            })
            .collect(),
    }
}

fn book_row_to_read_model(row: BookRow) -> BookReadModel {
    BookReadModel {
        id: row.id,
        series_id: row.series_id,
        series_title: row.series_title.clone(),
        series_title_sort: row.series_title,
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
        media_epub_divina_compatible: row.media_epub_divina_compatible,
        media_epub_is_kepub: row.media_epub_is_kepub,
        metadata_title: row.title,
        metadata_title_lock: row.metadata_title_lock,
        metadata_summary: row.metadata_summary,
        metadata_summary_lock: row.metadata_summary_lock,
        metadata_number: row.metadata_number,
        metadata_number_lock: row.metadata_number_lock,
        metadata_number_sort: row.metadata_number_sort,
        metadata_number_sort_lock: row.metadata_number_sort_lock,
        metadata_release_date: row.metadata_release_date,
        metadata_release_date_lock: row.metadata_release_date_lock,
        metadata_authors: row
            .metadata_authors
            .into_iter()
            .map(|a| BookMetadataAuthorReadModel {
                name: a.name,
                role: a.role,
            })
            .collect(),
        metadata_authors_lock: row.metadata_authors_lock,
        metadata_tags: row.metadata_tags,
        metadata_tags_lock: row.metadata_tags_lock,
        metadata_isbn: row.metadata_isbn,
        metadata_isbn_lock: row.metadata_isbn_lock,
        metadata_links: row
            .metadata_links
            .into_iter()
            .map(|l| BookMetadataLinkReadModel {
                label: l.label,
                url: l.url,
            })
            .collect(),
        metadata_links_lock: row.metadata_links_lock,
        metadata_created: row.metadata_created,
        metadata_last_modified: row.metadata_last_modified,
        read_progress: row.read_progress.map(|p| BookReadProgressReadModel {
            page: p.page,
            completed: p.completed,
            read_date: p.read_date,
            created: p.created,
            last_modified: p.last_modified,
            device_id: p.device_id,
            device_name: p.device_name,
        }),
        deleted: row.deleted,
        file_hash: row.file_hash,
        oneshot: row.oneshot,
    }
}

fn to_book_sort_mode(mode: &PersistedBooksSortMode) -> Option<BookSortMode> {
    Some(match mode {
        PersistedBooksSortMode::TitleAsc => BookSortMode::TitleAsc,
        PersistedBooksSortMode::TitleDesc => BookSortMode::TitleDesc,
        PersistedBooksSortMode::NameAsc => BookSortMode::NameAsc,
        PersistedBooksSortMode::NameDesc => BookSortMode::NameDesc,
        PersistedBooksSortMode::SeriesTitleAsc => BookSortMode::SeriesTitleAsc,
        PersistedBooksSortMode::SeriesTitleDesc => BookSortMode::SeriesTitleDesc,
        PersistedBooksSortMode::CreatedDateAsc => BookSortMode::CreatedDateAsc,
        PersistedBooksSortMode::CreatedDateDesc => BookSortMode::CreatedDateDesc,
        PersistedBooksSortMode::LastModifiedDateAsc => BookSortMode::LastModifiedDateAsc,
        PersistedBooksSortMode::LastModifiedDateDesc => BookSortMode::LastModifiedDateDesc,
        PersistedBooksSortMode::FileSizeAsc => BookSortMode::FileSizeAsc,
        PersistedBooksSortMode::FileSizeDesc => BookSortMode::FileSizeDesc,
        PersistedBooksSortMode::FileHashAsc => BookSortMode::FileHashAsc,
        PersistedBooksSortMode::FileHashDesc => BookSortMode::FileHashDesc,
        PersistedBooksSortMode::UrlAsc => BookSortMode::UrlAsc,
        PersistedBooksSortMode::UrlDesc => BookSortMode::UrlDesc,
        PersistedBooksSortMode::MediaStatusAsc => BookSortMode::MediaStatusAsc,
        PersistedBooksSortMode::MediaStatusDesc => BookSortMode::MediaStatusDesc,
        PersistedBooksSortMode::MediaCommentAsc => BookSortMode::MediaCommentAsc,
        PersistedBooksSortMode::MediaCommentDesc => BookSortMode::MediaCommentDesc,
        PersistedBooksSortMode::MediaTypeAsc => BookSortMode::MediaTypeAsc,
        PersistedBooksSortMode::MediaTypeDesc => BookSortMode::MediaTypeDesc,
        PersistedBooksSortMode::MediaPagesCountAsc => BookSortMode::MediaPagesCountAsc,
        PersistedBooksSortMode::MediaPagesCountDesc => BookSortMode::MediaPagesCountDesc,
        PersistedBooksSortMode::ReadProgressLastModifiedDateAsc => {
            BookSortMode::ReadProgressLastModifiedDateAsc
        }
        PersistedBooksSortMode::ReadProgressLastModifiedDateDesc => {
            BookSortMode::ReadProgressLastModifiedDateDesc
        }
        PersistedBooksSortMode::ReadProgressReadDateAsc => BookSortMode::ReadProgressReadDateAsc,
        PersistedBooksSortMode::ReadProgressReadDateDesc => BookSortMode::ReadProgressReadDateDesc,
        PersistedBooksSortMode::ReleaseDateAsc => BookSortMode::ReleaseDateAsc,
        PersistedBooksSortMode::ReleaseDateDesc => BookSortMode::ReleaseDateDesc,
        PersistedBooksSortMode::NumberSortAsc => BookSortMode::NumberSortAsc,
        PersistedBooksSortMode::NumberSortDesc => BookSortMode::NumberSortDesc,
        PersistedBooksSortMode::SeriesIdAsc => BookSortMode::SeriesIdAsc,
        PersistedBooksSortMode::ReadListNumberAsc => BookSortMode::ReadListNumberAsc,
        PersistedBooksSortMode::ReadListNumberDesc => BookSortMode::ReadListNumberDesc,
        PersistedBooksSortMode::RelevanceAsc => BookSortMode::RelevanceAsc,
        PersistedBooksSortMode::RelevanceDesc => BookSortMode::RelevanceDesc,
    })
}
