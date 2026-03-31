#![allow(clippy::too_many_arguments)]

use super::*;

pub async fn runtime_owned_persisted_series_page(
    database_file: &FsPath,
    context: &DiscoveryQueryContext,
    filters: &RuntimeSeriesFilters,
    sorts: &[String],
    full_text_search: Option<String>,
    page: usize,
    size: usize,
    unpaged: bool,
) -> Option<Result<PageEnvelope<PersistedSeriesSummary>, String>> {
    if !database_file.exists() {
        return None;
    }

    let sort_modes = parse_persisted_series_sort_modes(sorts);
    let has_persisted_rows = match persisted_series_exist(database_file).await {
        Ok(has_rows) => has_rows,
        Err(error) => return Some(Err(error)),
    };
    if !has_persisted_rows {
        return None;
    }

    Some(
        load_persisted_series_page(
            database_file,
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

pub fn parse_persisted_series_sort_modes(sorts: &[String]) -> Vec<PersistedSeriesSortMode> {
    let mut modes = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" => Some(PersistedSeriesSortMode::TitleAsc),
            "createdDate,desc" | "lastModifiedDate,desc" | "booksMetadata.releaseDate,desc" => {
                Some(PersistedSeriesSortMode::Latest)
            }
            "relevance,asc" => Some(PersistedSeriesSortMode::RelevanceAsc),
            "relevance,desc" => Some(PersistedSeriesSortMode::RelevanceDesc),
            _ => None,
        })
        .collect::<Vec<_>>();
    modes.dedup();
    if modes.is_empty() {
        modes.push(PersistedSeriesSortMode::TitleAsc);
    }
    modes
}

async fn persisted_series_exist(database_file: &FsPath) -> Result<bool, String> {
    persisted_backend_persisted_series_exist(database_file).await
}

pub async fn runtime_owned_series_list_response(
    headers: &HeaderMap,
    uri: &Uri,
    payload: Option<&Value>,
    full_text_search: Option<String>,
    auth_state: &DiscoveryAuthState,
    database_file: &FsPath,
    strict_runtime_shape: bool,
) -> Option<Response> {
    let query = uri.query().unwrap_or_default();
    let sorts = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .max(1);
    let unpaged = query_bool(query, "unpaged");

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
        restrict_series_filters_to_persisted_shape(&mut filters);
        filters.criteria.library_ids = remap_requested_library_ids_for_persisted(
            database_file,
            filters.criteria.library_ids.as_ref(),
        )
        .await;
    }

    let requested_library_ids = requested_library_ids_for_runtime_shape(
        strict_runtime_shape,
        filters.criteria.library_ids.clone(),
    );
    let context = match auth_state.resolve_query_context(headers, requested_library_ids.as_deref())
    {
        Some(context) => context,
        None => return Some(StatusCode::UNAUTHORIZED.into_response()),
    };

    if let Some(persisted_page) = runtime_owned_persisted_series_page(
        database_file,
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
                let mut response = Json(series_page_payload(page, !unpaged)).into_response();
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
