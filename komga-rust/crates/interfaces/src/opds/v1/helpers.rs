use super::super::feeds::{OpdsV1NavigationEntry, query_escape};

pub(super) fn series_feed_self_path(search: Option<&str>, publishers: &[String]) -> String {
    let mut query_parts = Vec::new();
    if let Some(search) = search {
        query_parts.push(format!("search={}", query_escape(search)));
    }
    for publisher in publishers {
        query_parts.push(format!("publisher={}", query_escape(publisher)));
    }

    if query_parts.is_empty() {
        "/opds/v1.2/series".to_string()
    } else {
        format!("/opds/v1.2/series?{}", query_parts.join("&"))
    }
}

pub(super) fn nav_entry_with_content(
    id: &str,
    title: &str,
    content: &str,
    href_path: &str,
) -> OpdsV1NavigationEntry {
    OpdsV1NavigationEntry {
        id: id.to_string(),
        title: title.to_string(),
        content: content.to_string(),
        href_path: href_path.to_string(),
        updated: None,
    }
}

pub(super) fn publisher_entry_id(publisher: &str) -> String {
    format!("publisher:{}", query_escape(publisher))
}

#[cfg(test)]
mod tests {
    use super::publisher_entry_id;

    #[test]
    fn publisher_entry_id_matches_kotlin_prefix_and_encoding() {
        assert_eq!(publisher_entry_id("ACME Press"), "publisher:ACME%20Press");
    }
}
