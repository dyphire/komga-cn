use std::io::Read;

use flate2::read::GzDecoder;
use serde_json::Value;

use crate::EpubParseError;

#[derive(Debug, Clone, PartialEq)]
pub struct EpubNavigation {
    pub positions: Vec<Value>,
    pub is_fixed_layout: bool,
    pub toc: Vec<EpubNavigationLink>,
    pub landmarks: Vec<EpubNavigationLink>,
    pub page_list: Vec<EpubNavigationLink>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EpubNavigationLink {
    pub title: Option<String>,
    pub href: Option<String>,
    pub children: Vec<EpubNavigationLink>,
}

pub fn decode_epub_navigation_extension(blob: &[u8]) -> Result<EpubNavigation, EpubParseError> {
    let mut decoder = GzDecoder::new(blob);
    let mut decoded = Vec::new();
    decoder
        .read_to_end(&mut decoded)
        .map_err(|error| EpubParseError::new(format!("decode epub extension blob: {error}")))?;
    let extension = serde_json::from_slice::<Value>(&decoded)
        .map_err(|error| EpubParseError::new(format!("parse epub extension blob json: {error}")))?;

    Ok(EpubNavigation {
        positions: extension
            .get("positions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default(),
        is_fixed_layout: extension
            .get("isFixedLayout")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        toc: navigation_links(&extension, "toc"),
        landmarks: navigation_links(&extension, "landmarks"),
        page_list: navigation_links(&extension, "pageList"),
    })
}

fn navigation_links(extension: &Value, field_name: &str) -> Vec<EpubNavigationLink> {
    extension
        .get(field_name)
        .and_then(Value::as_array)
        .map(|links| links.iter().filter_map(navigation_link).collect())
        .unwrap_or_default()
}

fn navigation_link(value: &Value) -> Option<EpubNavigationLink> {
    let entry = value.as_object()?;
    Some(EpubNavigationLink {
        title: entry
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string),
        href: entry
            .get("href")
            .and_then(Value::as_str)
            .map(str::to_string),
        children: entry
            .get("children")
            .and_then(Value::as_array)
            .map(|children| children.iter().filter_map(navigation_link).collect())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;
    use serde_json::json;
    use std::io::Write;

    #[test]
    fn decodes_gzipped_navigation_extension() {
        let payload = json!({
            "isFixedLayout": true,
            "positions": [{"href": "/chapter.xhtml"}],
            "toc": [{
                "title": "Chapter 1",
                "href": "/chapter.xhtml",
                "children": [{"title": "Part 1"}]
            }]
        });
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(payload.to_string().as_bytes())
            .expect("navigation payload should be writable");
        let blob = encoder
            .finish()
            .expect("navigation payload should finalize");

        let navigation = decode_epub_navigation_extension(&blob).expect("navigation should decode");
        assert!(navigation.is_fixed_layout);
        assert_eq!(navigation.positions.len(), 1);
        assert_eq!(navigation.toc[0].title.as_deref(), Some("Chapter 1"));
        assert_eq!(navigation.toc[0].children.len(), 1);
    }
}
