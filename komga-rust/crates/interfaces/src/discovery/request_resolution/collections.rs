use komga_application::discovery::CollectionListQuery;

use super::{query_bool, query_value, query_values};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCollectionListRequest {
    pub query: CollectionListQuery,
    pub requested_library_ids: Vec<String>,
}

pub fn resolve_collection_list_request(query: &str) -> ResolvedCollectionListRequest {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let search = query_value(query, "search")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let requested_library_ids = query_values(query, "library_id")
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let unpaged = query_bool(query, "unpaged");

    ResolvedCollectionListRequest {
        query: CollectionListQuery {
            page,
            size,
            unpaged,
            search,
        },
        requested_library_ids,
    }
}
