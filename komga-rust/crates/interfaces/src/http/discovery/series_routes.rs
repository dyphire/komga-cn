use super::*;
use crate::http::discovery;
use axum::extract::Path;

pub async fn series_alphabetical_groups_deprecated(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    if contains_legacy_search_query(query) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let mut conditions: Vec<Value> = vec![];

    push_series_any_of_string_conditions(&mut conditions, query, "library_id", "LibraryId");
    push_series_any_of_string_conditions(&mut conditions, query, "collection_id", "CollectionId");
    push_series_any_of_string_conditions(&mut conditions, query, "status", "SeriesStatus");
    push_series_any_of_string_conditions(&mut conditions, query, "read_status", "ReadStatus");
    push_series_any_of_string_conditions(&mut conditions, query, "publisher", "Publisher");
    push_series_any_of_string_conditions(&mut conditions, query, "language", "Language");
    push_series_any_of_string_conditions(&mut conditions, query, "genre", "Genre");
    push_series_any_of_string_conditions(&mut conditions, query, "tag", "Tag");
    push_series_any_of_string_conditions(&mut conditions, query, "sharing_label", "SharingLabel");

    let age_ratings = query_values(query, "age_rating");
    if !age_ratings.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": age_ratings
                .into_iter()
                .map(|value| {
                    let parsed = value.parse::<u16>().ok();
                    if let Some(parsed) = parsed {
                        json!({
                            "type": "AgeRating",
                            "operator": "is",
                            "value": parsed,
                        })
                    } else {
                        json!({
                            "type": "AgeRating",
                            "operator": "isNull",
                        })
                    }
                })
                .collect::<Vec<_>>(),
        }));
    }

    let release_years = query_values(query, "release_year");
    if !release_years.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": release_years
                .into_iter()
                .filter_map(|value| value.parse::<i32>().ok())
                .map(|year| {
                    let after = format!("{}-12-31T12:00:00Z", year - 1);
                    let before = format!("{}-01-01T12:00:00Z", year + 1);
                    json!({
                        "type": "AllOfSeries",
                        "conditions": [
                            {
                                "type": "ReleaseDate",
                                "operator": "after",
                                "value": after,
                            },
                            {
                                "type": "ReleaseDate",
                                "operator": "before",
                                "value": before,
                            }
                        ],
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let authors = query_values(query, "author");
    if !authors.is_empty() {
        conditions.push(json!({
            "type": "AnyOfSeries",
            "conditions": authors
                .into_iter()
                .map(author_query_to_series_condition)
                .collect::<Vec<_>>(),
        }));
    }

    if let Some(deleted) = query_bool(query, "deleted") {
        conditions.push(json!({
            "type": "Deleted",
            "operator": if deleted { "isTrue" } else { "isFalse" },
        }));
    }
    if let Some(complete) = query_bool(query, "complete") {
        conditions.push(json!({
            "type": "Complete",
            "operator": if complete { "isTrue" } else { "isFalse" },
        }));
    }
    if let Some(oneshot) = query_bool(query, "oneshot") {
        conditions.push(json!({
            "type": "OneShot",
            "operator": if oneshot { "isTrue" } else { "isFalse" },
        }));
    }

    let mut body = json!({
        "condition": {
            "type": "AllOfSeries",
            "conditions": conditions,
        }
    });

    if let Some(search) = query_value(query, "search").map(decode_query_component)
        && !search.trim().is_empty()
    {
        body["fullTextSearch"] = Value::String(search);
    }

    discovery::series_alphabetical_groups(
        headers,
        body,
        auth_state,
        auth_db.database_file.as_path(),
    )
    .await
}

pub async fn series_books(
    Extension(auth_db): Extension<AuthDatabaseState>,
    Extension(auth_state): Extension<DiscoveryAuthState>,
    headers: HeaderMap,
    Path(series_id): Path<String>,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let mut all_of = vec![json!({
        "type": "SeriesId",
        "operator": "is",
        "value": series_id,
    })];

    let media_statuses = query_values(query, "media_status");
    if !media_statuses.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": media_statuses
                .into_iter()
                .map(|value| {
                    json!({
                        "type": "MediaStatus",
                        "operator": "is",
                        "value": value,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let read_statuses = query_values(query, "read_status");
    if !read_statuses.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": read_statuses
                .into_iter()
                .map(|value| {
                    json!({
                        "type": "ReadStatus",
                        "operator": "is",
                        "value": value,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let tags = query_values(query, "tag");
    if !tags.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": tags
                .into_iter()
                .map(|value| {
                    json!({
                        "type": "Tag",
                        "operator": "is",
                        "value": value,
                    })
                })
                .collect::<Vec<_>>(),
        }));
    }

    let authors = query_values(query, "author");
    if !authors.is_empty() {
        all_of.push(json!({
            "type": "AnyOfBook",
            "conditions": authors
                .into_iter()
                .map(author_query_to_book_condition)
                .collect::<Vec<_>>(),
        }));
    }

    if let Some(deleted) = query_bool(query, "deleted") {
        all_of.push(json!({
            "type": "Deleted",
            "operator": if deleted { "isTrue" } else { "isFalse" },
        }));
    }

    let body = json!({
        "condition": {
            "type": "AllOfBook",
            "conditions": all_of,
        }
    });

    let request_uri = ensure_sort_query(uri, "series,metadata.numberSort,asc");

    discovery::books_list(
        Extension(auth_db),
        Extension(auth_state),
        headers,
        request_uri,
        Bytes::from(body.to_string()),
    )
    .await
}

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let (candidate_key, value) = pair.split_once('=')?;
        (candidate_key == key).then_some(value)
    })
}

fn query_values(query: &str, key: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|pair| {
            let (candidate_key, value) = pair.split_once('=')?;
            (candidate_key == key).then_some(value.replace('+', " "))
        })
        .filter(|value| !value.trim().is_empty())
        .collect()
}

fn query_bool(query: &str, key: &str) -> Option<bool> {
    query_value(query, key).and_then(|value| {
        if value.eq_ignore_ascii_case("true") {
            Some(true)
        } else if value.eq_ignore_ascii_case("false") {
            Some(false)
        } else {
            None
        }
    })
}

fn push_series_any_of_string_conditions(
    conditions: &mut Vec<Value>,
    query: &str,
    key: &str,
    condition_type: &str,
) {
    let values = query_values(query, key);
    if values.is_empty() {
        return;
    }

    conditions.push(json!({
        "type": "AnyOfSeries",
        "conditions": values
            .into_iter()
            .map(|value| {
                json!({
                    "type": condition_type,
                    "operator": "is",
                    "value": value,
                })
            })
            .collect::<Vec<_>>(),
    }));
}

pub(super) fn author_query_to_author_match(value: String) -> Value {
    let normalized = decode_query_component(&value);
    let (name, role) = normalized
        .split_once(',')
        .map(|(name, role)| (name.trim(), role.trim()))
        .unwrap_or((normalized.trim(), ""));

    let mut payload = serde_json::Map::new();
    if !name.is_empty() {
        payload.insert("name".to_string(), Value::String(name.to_string()));
    }
    if !role.is_empty() {
        payload.insert("role".to_string(), Value::String(role.to_string()));
    }

    Value::Object(payload)
}

fn author_query_to_series_condition(value: String) -> Value {
    json!({
        "type": "Author",
        "operator": "is",
        "value": author_query_to_author_match(value),
    })
}

fn author_query_to_book_condition(value: String) -> Value {
    json!({
        "type": "Author",
        "operator": "is",
        "value": author_query_to_author_match(value),
    })
}

pub(super) fn ensure_sort_query(uri: Uri, default_sort: &str) -> Uri {
    if uri
        .query()
        .is_some_and(|query| query.split('&').any(|pair| pair.starts_with("sort=")))
    {
        return uri;
    }

    let path = uri.path();
    let next_query = match uri.query() {
        Some(query) if !query.is_empty() => format!("{query}&sort={default_sort}"),
        _ => format!("sort={default_sort}"),
    };

    Uri::builder()
        .path_and_query(format!("{path}?{next_query}"))
        .build()
        .unwrap_or(uri)
}

pub(super) fn decode_query_component(value: &str) -> String {
    let mut decoded = String::with_capacity(value.len());
    let mut bytes = value.as_bytes().iter().copied();
    while let Some(byte) = bytes.next() {
        if byte == b'%' {
            let high = bytes.next();
            let low = bytes.next();
            if let (Some(high), Some(low)) = (high, low) {
                let hex = [high, low];
                if let Ok(hex) = std::str::from_utf8(&hex)
                    && let Ok(parsed) = u8::from_str_radix(hex, 16)
                {
                    decoded.push(parsed as char);
                    continue;
                }
            }
            decoded.push('%');
            if let Some(high) = high {
                decoded.push(high as char);
            }
            if let Some(low) = low {
                decoded.push(low as char);
            }
            continue;
        }
        if byte == b'+' {
            decoded.push(' ');
        } else {
            decoded.push(byte as char);
        }
    }
    decoded
}

#[cfg(test)]
mod tests {
    use super::{author_query_to_author_match, decode_query_component, ensure_sort_query};
    use axum::http::Uri;

    #[test]
    fn decode_query_component_decodes_plus_and_percent_encoding() {
        let decoded = decode_query_component("John+Doe%2Cwriter%20team");
        assert_eq!(decoded, "John Doe,writer team");
    }

    #[test]
    fn ensure_sort_query_appends_default_sort_when_missing() {
        let uri: Uri = "/api/v1/series/series-1/books?page=2"
            .parse()
            .expect("uri should parse");
        let updated = ensure_sort_query(uri, "series,metadata.numberSort,asc");
        assert_eq!(
            updated.to_string(),
            "/api/v1/series/series-1/books?page=2&sort=series,metadata.numberSort,asc",
        );
    }

    #[test]
    fn ensure_sort_query_keeps_existing_sort() {
        let uri: Uri = "/api/v1/series/series-1/books?sort=createdDate,desc"
            .parse()
            .expect("uri should parse");
        let updated = ensure_sort_query(uri.clone(), "series,metadata.numberSort,asc");
        assert_eq!(updated, uri);
    }

    #[test]
    fn author_query_to_author_match_splits_name_and_role() {
        let parsed = author_query_to_author_match("Jane+Doe,writer".to_string());
        assert_eq!(parsed["name"], "Jane Doe");
        assert_eq!(parsed["role"], "writer");
    }
}
