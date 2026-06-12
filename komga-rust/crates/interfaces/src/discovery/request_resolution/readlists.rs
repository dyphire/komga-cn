use super::{decode_query_component, query_bool, query_value, query_values};
use komga_application::discovery::{ReadListBooksQuery, ReadListsQuery, ReadListsSort};
use komga_domain::discovery::DiscoveryError;

use super::filter_values::{parse_media_status_values, parse_read_status_values};

pub fn normalize_readlists_search(search: Option<String>) -> Option<String> {
    search.and_then(|value| (!value.trim().is_empty()).then_some(value))
}

pub fn parse_readlists_sort(value: &str) -> ReadListsSort {
    let mut parts = value.splitn(2, ',');
    let field = parts.next().unwrap_or_default().trim();
    let direction = parts.next().unwrap_or("asc").trim();

    if field.eq_ignore_ascii_case("name") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::NameDesc
        } else {
            ReadListsSort::NameAsc
        }
    } else if field.eq_ignore_ascii_case("createdDate") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::CreatedDateDesc
        } else {
            ReadListsSort::CreatedDateAsc
        }
    } else if field.eq_ignore_ascii_case("lastModifiedDate") {
        if direction.eq_ignore_ascii_case("desc") {
            ReadListsSort::LastModifiedDateDesc
        } else {
            ReadListsSort::LastModifiedDateAsc
        }
    } else {
        ReadListsSort::SearchOrName
    }
}

pub fn resolve_readlists_query(query: &str) -> ReadListsQuery {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let library_ids = {
        let values = query_values(query, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };
    let search_values = query_values(query, "search")
        .into_iter()
        .map(decode_query_component)
        .collect::<Vec<_>>();
    let search = normalize_readlists_search(match search_values.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        _ => Some(search_values.join(",")),
    });
    let sort = query_values(query, "sort")
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(parse_readlists_sort)
        .unwrap_or(ReadListsSort::SearchOrName);

    ReadListsQuery {
        page,
        size,
        unpaged: query_bool(query, "unpaged"),
        library_ids,
        search,
        sort,
    }
}

pub fn resolve_readlist_books_query(
    readlist_id: impl Into<String>,
    query: &str,
) -> Result<ReadListBooksQuery, DiscoveryError> {
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let library_ids = {
        let values = query_values(query, "library_id")
            .into_iter()
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        (!values.is_empty()).then_some(values)
    };

    Ok(ReadListBooksQuery {
        readlist_id: readlist_id.into(),
        page,
        size,
        unpaged: query_bool(query, "unpaged"),
        library_ids,
        deleted: query_value(query, "deleted").map(|value| value.eq_ignore_ascii_case("true")),
        tags: decoded_query_values(query, "tag"),
        read_statuses: decoded_query_values(query, "read_status")
            .map(|values| parse_read_status_values(values, "ReadStatus"))
            .transpose()?,
        media_statuses: decoded_query_values(query, "media_status")
            .map(|values| parse_media_status_values(values, "MediaStatus"))
            .transpose()?,
        authors: decoded_query_values(query, "author"),
    })
}

fn decoded_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(decode_query_component)
        .filter(|value| !value.trim().is_empty())
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

#[cfg(test)]
mod tests {
    use super::normalize_readlists_search;

    #[test]
    fn normalize_readlists_search_returns_none_for_blank_effective_values() {
        assert_eq!(normalize_readlists_search(None), None);
        assert_eq!(normalize_readlists_search(Some(String::new())), None);
        assert_eq!(
            normalize_readlists_search(Some("   \t\n".to_string())),
            None
        );
    }

    #[test]
    fn normalize_readlists_search_preserves_non_blank_value_without_trimming() {
        let decoded = " alpha ".to_string();

        assert_eq!(
            normalize_readlists_search(Some(decoded.clone())),
            Some(decoded),
        );
    }
}
