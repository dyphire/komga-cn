use komga_application::discovery::{CollectionListQuery, CollectionsSort};

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
    let sort = query_values(query, "sort")
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(parse_collections_sort)
        .unwrap_or(CollectionsSort::SearchOrName);

    ResolvedCollectionListRequest {
        query: CollectionListQuery {
            page,
            size,
            unpaged,
            search,
            sort,
        },
        requested_library_ids,
    }
}

pub fn parse_collections_sort(value: &str) -> CollectionsSort {
    let mut parts = value.splitn(2, ',');
    let field = parts.next().unwrap_or_default().trim();
    let direction = parts.next().unwrap_or("asc").trim();

    if field.eq_ignore_ascii_case("name") {
        if direction.eq_ignore_ascii_case("desc") {
            CollectionsSort::NameDesc
        } else {
            CollectionsSort::NameAsc
        }
    } else if field.eq_ignore_ascii_case("createdDate") {
        if direction.eq_ignore_ascii_case("desc") {
            CollectionsSort::CreatedDateDesc
        } else {
            CollectionsSort::CreatedDateAsc
        }
    } else if field.eq_ignore_ascii_case("lastModifiedDate") {
        if direction.eq_ignore_ascii_case("desc") {
            CollectionsSort::LastModifiedDateDesc
        } else {
            CollectionsSort::LastModifiedDateAsc
        }
    } else {
        CollectionsSort::SearchOrName
    }
}
