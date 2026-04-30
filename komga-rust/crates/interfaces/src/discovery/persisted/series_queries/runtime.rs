#![allow(clippy::too_many_arguments)]

use super::common_helpers::runtime_list_request;
use crate::discovery_auth::state::DiscoveryAuthState;
use crate::state::PersistedDiscoveryService;

use super::*;
use crate::discovery::filters::{
    OperatorValidationMode, parse_runtime_series_filters_with_mode,
    webui_bridge_series_filters_from_payload,
};

pub async fn runtime_owned_persisted_series_page(
    backend: &dyn PersistedDiscoveryService,
    context: &DiscoveryQueryContext,
    filters: &RuntimeSeriesFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Option<Result<PageEnvelope<PersistedSeriesSummary>, String>> {
    let sort_modes = parse_persisted_series_sort_modes(sorts, full_text_search.as_deref());
    let has_persisted_rows = match persisted_series_exist(backend).await {
        Ok(has_rows) => has_rows,
        Err(error) => return Some(Err(error)),
    };
    if !has_persisted_rows {
        return Some(Ok(PageEnvelope::from_slice(vec![], page, size, 0)));
    }

    Some(
        load_persisted_series_page(
            backend,
            context,
            PersistedSeriesBrowseQuery::from_runtime_filters(
                filters,
                full_text_search,
                page,
                size,
                unpaged,
                sort_modes,
            ),
        )
        .await,
    )
}

pub fn parse_persisted_series_sort_modes(
    sorts: &[String],
    full_text_search: Option<&str>,
) -> Vec<PersistedSeriesSortMode> {
    let has_full_text_search = full_text_search
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_some();
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" | "titleSort,asc" => Some(PersistedSeriesSortMode::TitleAsc),
            "metadata.titleSort,desc" | "titleSort,desc" => {
                Some(PersistedSeriesSortMode::TitleDesc)
            }
            "name,asc" => Some(PersistedSeriesSortMode::NameAsc),
            "name,desc" => Some(PersistedSeriesSortMode::NameDesc),
            "readDate,asc" => Some(PersistedSeriesSortMode::ReadDateAsc),
            "readDate,desc" => Some(PersistedSeriesSortMode::ReadDateDesc),
            "collection.number,asc" => Some(PersistedSeriesSortMode::CollectionNumberAsc),
            "collection.number,desc" => Some(PersistedSeriesSortMode::CollectionNumberDesc),
            "random,asc" | "random,desc" => Some(PersistedSeriesSortMode::Random),
            "createdDate,asc" | "created,asc" => Some(PersistedSeriesSortMode::CreatedAsc),
            "createdDate,desc" | "created,desc" => Some(PersistedSeriesSortMode::CreatedDesc),
            "lastModifiedDate,asc" | "lastModified,asc" => {
                Some(PersistedSeriesSortMode::LastModifiedAsc)
            }
            "lastModifiedDate,desc" | "lastModified,desc" => {
                Some(PersistedSeriesSortMode::LastModifiedDesc)
            }
            "booksMetadata.releaseDate,asc" => Some(PersistedSeriesSortMode::ReleaseDateAsc),
            "booksMetadata.releaseDate,desc" => Some(PersistedSeriesSortMode::ReleaseDateDesc),
            "booksCount,asc" => Some(PersistedSeriesSortMode::BooksCountAsc),
            "booksCount,desc" => Some(PersistedSeriesSortMode::BooksCountDesc),
            "relevance,asc" if has_full_text_search => Some(PersistedSeriesSortMode::RelevanceAsc),
            "relevance,desc" if has_full_text_search => {
                Some(PersistedSeriesSortMode::RelevanceDesc)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() && sorts.is_empty() && has_full_text_search {
        modes.push(PersistedSeriesSortMode::RelevanceAsc);
    }
    modes
}

async fn persisted_series_exist(backend: &dyn PersistedDiscoveryService) -> Result<bool, String> {
    backend.persisted_series_exist().await
}

pub async fn runtime_owned_series_list_response(
    backend: &dyn PersistedDiscoveryService,
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    identity: &dyn crate::state::IdentityService,
    strict_runtime_shape: bool,
) -> Option<Response> {
    let query = uri.query().unwrap_or_default();
    let request = runtime_list_request(query);
    let sorts = request.sorts;
    let resolved_sort_modes =
        parse_persisted_series_sort_modes(&sorts, full_text_search.as_deref());
    let page = request.page;
    let size = request.size;
    let unpaged = request.unpaged;

    let parse_mode = OperatorValidationMode::from(query_validation_mode(strict_runtime_shape));
    let mut filters = match parse_runtime_series_filters_with_mode(
        payload.and_then(|value| value.get("condition")),
        parse_mode,
    ) {
        Ok(filters) => filters,
        Err(error) => {
            if strict_runtime_shape {
                return Some(invalid_runtime_series_list_response(error));
            } else {
                webui_bridge_series_filters_from_payload(payload)
            }
        }
    };

    if !strict_runtime_shape {
        filters.criteria.library_ids = remap_requested_library_ids_for_persisted(
            backend,
            filters.criteria.library_ids.as_ref(),
        )
        .await;
    }

    let requested_library_ids = requested_library_ids_for_runtime_shape(
        strict_runtime_shape,
        filters.criteria.library_ids.clone(),
    );
    let context = match auth_state
        .resolve_query_context_with_persistence(identity, headers, requested_library_ids.as_deref())
        .await
    {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(persisted_page) = runtime_owned_persisted_series_page(
        backend,
        &context,
        &filters,
        &sorts,
        full_text_search.clone(),
        page,
        size,
        unpaged,
    )
    .await
    {
        match persisted_page {
            Ok(page) => {
                let mut response = Json(series_page_payload(
                    page,
                    !unpaged,
                    !resolved_sort_modes.is_empty(),
                ))
                .into_response();
                mark_runtime_owned(&mut response);
                return Some(response);
            }
            Err(error) => {
                return Some(
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(json!({ "error": format!("runtime series list failed: {error}") })),
                    )
                        .into_response(),
                );
            }
        }
    }

    None
}
