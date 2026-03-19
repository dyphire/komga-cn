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

    if let Some(remainder) = value.strip_prefix(base_url) {
        if remainder.is_empty()
            || matches!(
                remainder.as_bytes().first(),
                Some(b'/') | Some(b'?') | Some(b'#')
            )
        {
            return format!("{CANONICAL_ORIGIN}{remainder}");
        }
    }

    value.to_string()
}

#[cfg(test)]
mod tests {
    use super::normalize_json_body;

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
}
