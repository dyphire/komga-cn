pub mod access_log;
pub mod cache;
pub mod discovery;
pub mod discovery_auth;
pub mod helpers;
pub mod identity_access;
pub mod library_catalog;
pub mod media_assets;
pub mod opds;
pub mod operational;
pub mod request_urls;
pub mod router;
pub mod state;

pub const CACHE_CONTROL_PRIVATE: &str = "max-age=0, must-revalidate, private";
pub const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-runtime-search-ownership";
pub const PERSISTED_OWNERSHIP_MARKER: &str = "persisted-owned-writer";
