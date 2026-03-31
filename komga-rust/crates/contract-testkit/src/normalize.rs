use anyhow::Context;
use reqwest::header::HeaderMap;
use std::collections::{BTreeMap, BTreeSet};

pub fn normalize_headers(
    headers: &HeaderMap,
    allowlist: &BTreeSet<String>,
) -> BTreeMap<String, Vec<String>> {
    let mut normalized = BTreeMap::new();

    for (name, value) in headers {
        let header_name = name.as_str().to_ascii_lowercase();
        if !allowlist.contains(&header_name) {
            continue;
        }

        normalized
            .entry(header_name)
            .or_insert_with(Vec::new)
            .push(value.to_str().unwrap_or_default().to_string());
    }

    normalized
}

pub fn normalize_json_body(body: &str, base_url: &str) -> anyhow::Result<serde_json::Value> {
    let value: serde_json::Value =
        serde_json::from_str(body).context("failed to parse json body")?;
    Ok(sort_value(value, base_url))
}

pub fn normalize_xml_body(body: &str, base_url: &str) -> String {
    let canonical_origin_body = normalize_xml_origin(body, base_url);
    normalize_xml_timestamps(&canonical_origin_body)
}

const CANONICAL_ORIGIN: &str = "http://komga.local";

fn sort_value(value: serde_json::Value, base_url: &str) -> serde_json::Value {
    match value {
        serde_json::Value::Array(items) => serde_json::Value::Array(
            items
                .into_iter()
                .map(|item| sort_value(item, base_url))
                .collect(),
        ),
        serde_json::Value::Object(map) => {
            let sorted = map
                .into_iter()
                .map(|(key, value)| (key, sort_value(value, base_url)))
                .collect::<serde_json::Map<String, serde_json::Value>>();
            serde_json::Value::Object(sorted)
        }
        serde_json::Value::String(value) => {
            serde_json::Value::String(normalize_url(&value, base_url))
        }
        primitive => primitive,
    }
}

fn normalize_url(value: &str, base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');

    if let Some(remainder) = value.strip_prefix(base_url)
        && (remainder.is_empty()
            || matches!(
                remainder.as_bytes().first(),
                Some(b'/') | Some(b'?') | Some(b'#')
            ))
    {
        return format!("{CANONICAL_ORIGIN}{remainder}");
    }

    value.to_string()
}

fn normalize_xml_origin(body: &str, base_url: &str) -> String {
    let base_url = base_url.trim_end_matches('/');
    let normalized = body.replace("\r\n", "\n");

    if base_url.is_empty() {
        normalized
    } else {
        normalized.replace(base_url, CANONICAL_ORIGIN)
    }
}

fn normalize_xml_timestamps(body: &str) -> String {
    replace_tag_content(body, "updated", "__OPDS_UPDATED__")
}

fn replace_tag_content(body: &str, tag_name: &str, replacement: &str) -> String {
    let open_tag = format!("<{tag_name}>");
    let close_tag = format!("</{tag_name}>");

    let mut cursor = 0usize;
    let mut output = String::with_capacity(body.len());

    while let Some(open_relative) = body[cursor..].find(&open_tag) {
        let open_index = cursor + open_relative;
        output.push_str(&body[cursor..open_index + open_tag.len()]);

        let content_start = open_index + open_tag.len();
        if let Some(close_relative) = body[content_start..].find(&close_tag) {
            let close_index = content_start + close_relative;
            output.push_str(replacement);
            output.push_str(&body[close_index..close_index + close_tag.len()]);
            cursor = close_index + close_tag.len();
        } else {
            output.push_str(&body[content_start..]);
            cursor = body.len();
            break;
        }
    }

    if cursor < body.len() {
        output.push_str(&body[cursor..]);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::{normalize_json_body, normalize_xml_body};

    #[test]
    fn normalize_json_body_rewrites_service_local_absolute_urls() {
        let body = r#"{
            "local": "http://127.0.0.1:25610/opds/v2/books/book-1/manifest",
            "external": "https://readium.org/webpub-manifest/context.jsonld"
        }"#;

        let normalized = normalize_json_body(body, "http://127.0.0.1:25610")
            .expect("json body should normalize");

        assert_eq!(
            normalized.get("local"),
            Some(&serde_json::Value::String(
                "http://komga.local/opds/v2/books/book-1/manifest".to_string(),
            ))
        );
        assert_eq!(
            normalized.get("external"),
            Some(&serde_json::Value::String(
                "https://readium.org/webpub-manifest/context.jsonld".to_string(),
            ))
        );
    }

    #[test]
    fn normalize_xml_body_rewrites_base_origin_and_updated_timestamps() {
        let body = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed>
  <id>allSeries</id>
  <updated>2026-03-20T12:34:56.789012Z</updated>
  <link href="http://127.0.0.1:25610/opds/v1.2/series"/>
  <entry>
    <updated>2024-01-02T08:04:05Z</updated>
  </entry>
</feed>"#;

        let normalized = normalize_xml_body(body, "http://127.0.0.1:25610");

        assert!(normalized.contains("http://komga.local/opds/v1.2/series"));
        assert_eq!(normalized.matches("__OPDS_UPDATED__").count(), 2);
        assert!(!normalized.contains("2026-03-20T12:34:56.789012Z"));
        assert!(!normalized.contains("2024-01-02T08:04:05Z"));
    }
}
