use std::collections::HashMap;

use komga_application::discovery::browse_engine::{
    self,
    models::{
        AgeRestrictionKind as EngineAgeRestrictionKind, AuthorEntry, BookBrowseQuery,
        BookEvaluationContext, BookPosterRow, BookRow, BookSortMode, BrowseContext,
        BrowseRestrictions, ReadProgressRow, WebLinkEntry,
    },
};
use komga_application::discovery::{
    BookMetadataAuthorReadModel, BookMetadataLinkReadModel, BookReadProgressReadModel,
};
use komga_domain::discovery::{
    BookCondition, BookValueCondition, InclusionCondition, PageEnvelope,
};

use super::models::{PersistedBookSummary, PersistedBooksBrowseQuery, PersistedBooksSortMode};
use super::{DiscoveryQueryContext, PersistedDiscoveryBrowseDataSource};

use komga_application::discovery::BookReadModel;

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

pub(crate) async fn load_persisted_books_page(
    backend: &dyn PersistedDiscoveryBrowseDataSource,
    context: &DiscoveryQueryContext,
    query: PersistedBooksBrowseQuery,
) -> Result<PageEnvelope<BookReadModel>, String> {
    let mut books = Vec::new();
    let mut relevance_ranks: HashMap<String, usize> = HashMap::new();

    if let Some(search) = query.search.as_ref().map(|value| value.trim())
        && !search.is_empty()
    {
        let total_count = backend.load_persisted_book_count().await?;
        let candidate_ids = backend.search_book_ids(search, total_count.max(1)).await?;
        if !candidate_ids.is_empty() {
            relevance_ranks = candidate_ids
                .iter()
                .enumerate()
                .map(|(index, id)| (id.clone(), index))
                .collect();
            books = backend
                .load_persisted_book_summaries_by_ids(context.user_id.as_deref(), &candidate_ids)
                .await?;
        }
    } else {
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
    let rows: Vec<BookRow> = books.into_iter().map(to_book_row).collect();

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

    let page = browse_engine::filter_and_paginate_books(rows, &browse_ctx, engine_query, eval_ctx)?;

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
// PLACEHOLDER_HELPERS

async fn build_book_eval_context(
    backend: &dyn PersistedDiscoveryBrowseDataSource,
    context: &DiscoveryQueryContext,
    condition: Option<&BookCondition>,
) -> Result<BookEvaluationContext, String> {
    let (readlist_memberships, posters, release_date_cutoffs) = match condition {
        Some(condition) => {
            let readlist_memberships =
                if browse_engine::book_condition_needs_readlist_memberships(condition) {
                    Some(backend.load_readlist_memberships().await?)
                } else {
                    None
                };
            let posters = if browse_engine::book_condition_needs_posters(condition) {
                Some(
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
                )
            } else {
                None
            };
            let offsets = browse_engine::collect_book_release_date_offsets(condition);
            let mut cutoffs = HashMap::new();
            for days in offsets {
                cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
            }
            (readlist_memberships, posters, cutoffs)
        }
        None => (None, None, HashMap::new()),
    };

    Ok(BookEvaluationContext {
        user_id_present: context.user_id.is_some(),
        readlist_memberships,
        posters,
        release_date_cutoffs,
    })
}

fn to_browse_context(context: &DiscoveryQueryContext) -> BrowseContext {
    BrowseContext {
        user_id: context.user_id.clone(),
        is_admin: context.is_admin,
        authorized_library_ids: context.authorized_library_ids.clone(),
        restrictions: context.restrictions.as_ref().map(|r| BrowseRestrictions {
            age: r.age,
            age_restriction: r.age_restriction.map(|kind| match kind {
                komga_domain::discovery::AgeRestrictionKind::Exclude => {
                    EngineAgeRestrictionKind::Exclude
                }
                komga_domain::discovery::AgeRestrictionKind::AllowOnly => {
                    EngineAgeRestrictionKind::AllowOnly
                }
            }),
            labels_allow: r.labels_allow.clone(),
            labels_exclude: r.labels_exclude.clone(),
        }),
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
// PLACEHOLDER_MAPPINGS

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
