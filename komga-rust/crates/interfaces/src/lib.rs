mod discovery_detail_access;
pub mod http;
pub mod media_assets_runtime_access;
pub mod opds_catalog_access;
pub mod opds_persisted_access;
pub mod operational_runtime_access;
pub mod operational_settings_access;
pub mod runtime_identity_access;

pub const CACHE_CONTROL_PRIVATE: &str = "max-age=0, must-revalidate, private";
pub const SEARCH_OWNERSHIP_HEADER: &str = "x-komga-runtime-search-ownership";
pub const PERSISTED_OWNERSHIP_MARKER: &str = "persisted-owned-writer";
