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

fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

fn query_values<'a>(query: &'a str, key: &str) -> Vec<&'a str> {
    query
        .split('&')
        .filter_map(|pair| {
            let mut parts = pair.splitn(2, '=');
            let name = parts.next().unwrap_or_default();
            if name != key {
                return None;
            }
            Some(parts.next().unwrap_or_default())
        })
        .collect()
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
