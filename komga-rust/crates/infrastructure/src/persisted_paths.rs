use std::path::{Path, PathBuf};

use reqwest::Url;

pub(crate) fn resolve_stored_path(value: &str) -> PathBuf {
    resolve_file_url_path(value).unwrap_or_else(|| PathBuf::from(value))
}

pub(crate) fn resolve_rooted_path(root: &Path, stored_path: &str) -> PathBuf {
    if let Some(path) = resolve_file_url_path(stored_path) {
        return path;
    }

    let stored_path = PathBuf::from(stored_path);
    if stored_path.is_absolute() {
        stored_path
    } else {
        root.join(stored_path)
    }
}

pub(crate) fn resolve_library_item_path(library_root: &str, stored_path: &str) -> PathBuf {
    let root = resolve_stored_path(library_root);
    resolve_rooted_path(root.as_path(), stored_path)
}

pub(crate) fn resolve_optional_library_item_path(
    library_root: Option<&str>,
    stored_path: &str,
) -> Option<PathBuf> {
    if resolve_file_url_path(stored_path).is_some() || Path::new(stored_path).is_absolute() {
        return Some(resolve_stored_path(stored_path));
    }

    library_root.map(|root| resolve_library_item_path(root, stored_path))
}

fn resolve_file_url_path(value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if !trimmed.starts_with("file:") {
        return None;
    }

    Url::parse(trimmed)
        .ok()
        .filter(|url| url.scheme() == "file")
        .and_then(|url| url.to_file_path().ok())
        .or_else(|| decode_legacy_file_url_path(trimmed))
}

fn decode_legacy_file_url_path(value: &str) -> Option<PathBuf> {
    let raw_path = value.strip_prefix("file:")?;
    let decoded = percent_decode(raw_path)?;
    if decoded.is_empty() {
        return None;
    }

    Some(PathBuf::from(decoded))
}

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hi = *bytes.get(index + 1)?;
            let lo = *bytes.get(index + 2)?;
            decoded.push((hex_value(hi)? << 4) | hex_value(lo)?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }

    String::from_utf8(decoded).ok()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        resolve_library_item_path, resolve_optional_library_item_path, resolve_rooted_path,
        resolve_stored_path,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn resolve_library_item_path_prefers_absolute_file_url_from_book_row() {
        assert_eq!(
            resolve_library_item_path(
                "file:/library%20root",
                "file:/elsewhere/books/fixture%20book.cbz",
            ),
            PathBuf::from("/elsewhere/books/fixture book.cbz")
        );
    }

    #[test]
    fn resolve_rooted_path_joins_relative_paths_against_decoded_root() {
        let root = resolve_stored_path("file:/library%20root");
        assert_eq!(
            resolve_rooted_path(root.as_path(), "books/fixture-book.cbz"),
            Path::new("/library root/books/fixture-book.cbz")
        );
    }

    #[test]
    fn resolve_optional_library_item_path_rejects_relative_path_without_root() {
        assert_eq!(
            resolve_optional_library_item_path(None, "sidecars/cover.jpg"),
            None
        );
    }
}
