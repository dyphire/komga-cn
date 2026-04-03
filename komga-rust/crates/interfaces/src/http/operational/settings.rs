use std::path::PathBuf;

#[path = "settings/announcements.rs"]
mod announcements;
#[path = "settings/claims.rs"]
mod claims;
#[path = "settings/client_settings.rs"]
mod client_settings;
#[path = "settings/filesystem.rs"]
mod filesystem;
#[path = "settings/fonts.rs"]
mod fonts;
#[path = "settings/operations.rs"]
mod operations;
#[path = "settings/page_hashes.rs"]
mod page_hashes;
#[path = "settings/server_settings.rs"]
mod server_settings;
#[path = "settings/transient_books.rs"]
mod transient_books;

pub(crate) use announcements::{get_announcements, get_releases, put_announcements};
pub(crate) use claims::{get_claim_status, post_claim};
pub(crate) use client_settings::{
    delete_client_settings_global, delete_client_settings_user, get_client_settings_global,
    get_client_settings_user, patch_client_settings_global, patch_client_settings_user,
};
pub(crate) use filesystem::post_filesystem;
pub(crate) use fonts::{get_font_family_css, get_font_file, get_fonts_families};
pub(crate) use operations::{
    delete_syncpoints_me, delete_tasks, get_history, get_oauth2_providers,
};
pub(crate) use page_hashes::{
    get_page_hash_matches, get_page_hash_thumbnail, get_page_hash_unknown_thumbnail,
    get_page_hashes, get_page_hashes_unknown, post_page_hash_delete_all,
    post_page_hash_delete_match, put_page_hash,
};
pub(crate) use server_settings::{get_server_settings, update_server_settings};
pub(crate) use transient_books::{
    get_transient_book_page, post_transient_book_analyze, post_transient_books,
};

fn query_value(query: &str, key: &str) -> Option<String> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(decode_query_component(parts.next().unwrap_or_default()))
    })
}

fn query_values(query: &str, key: &str) -> Vec<String> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            Some(decode_query_component(parts.next().unwrap_or_default()))
        })
        .collect()
}

fn decode_query_component(value: &str) -> String {
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
    use super::{decode_query_component, query_value, query_values};

    #[test]
    fn query_helpers_decode_percent_encoded_values() {
        assert_eq!(
            query_value("sort=pageNumber%2Cdesc", "sort"),
            Some("pageNumber,desc".to_string())
        );
        assert_eq!(
            query_values("sort=bookId%2Casc&sort=pageNumber%2Cdesc", "sort"),
            vec!["bookId,asc".to_string(), "pageNumber,desc".to_string()]
        );
    }

    #[test]
    fn decode_query_component_decodes_plus_and_percent_sequences() {
        assert_eq!(
            decode_query_component("hello+world%2Cteam"),
            "hello world,team"
        );
    }
}

fn normalize_requested_path(requested_path: &str, runtime_config_dir: Option<&PathBuf>) -> PathBuf {
    let raw = PathBuf::from(requested_path);
    let candidate = if raw.is_absolute() {
        raw
    } else if let Some(config_dir) = runtime_config_dir {
        config_dir.join(raw)
    } else {
        raw
    };

    candidate.canonicalize().unwrap_or(candidate)
}
