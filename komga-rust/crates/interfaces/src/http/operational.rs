#[path = "operational/actuator.rs"]
mod actuator;
#[path = "operational/cors.rs"]
mod cors;
#[path = "operational/helpers.rs"]
mod helpers;
#[path = "operational/settings.rs"]
mod settings;
#[path = "operational/sse.rs"]
mod sse;
#[path = "operational/webui.rs"]
mod webui;
#[path = "operational/webui_assets.rs"]
mod webui_assets;

pub(super) use actuator::{
    actuator_health, actuator_info, actuator_logfile, actuator_metric_detail,
    actuator_metrics_index, actuator_root, actuator_shutdown,
};
pub(super) use cors::dev_cors_middleware;
pub(super) use settings::{
    delete_client_settings_global, delete_client_settings_user, delete_syncpoints_me, delete_tasks,
    get_announcements, get_claim_status, get_client_settings_global, get_client_settings_user,
    get_font_family_css, get_font_file, get_fonts_families, get_history, get_oauth2_providers,
    get_page_hash_matches, get_page_hash_thumbnail, get_page_hash_unknown_thumbnail,
    get_page_hashes, get_page_hashes_unknown, get_releases, get_server_settings,
    get_transient_book_page, patch_client_settings_global, patch_client_settings_user, post_claim,
    post_filesystem, post_page_hash_delete_all, post_page_hash_delete_match,
    post_transient_book_analyze, post_transient_books, put_announcements, put_page_hash,
    update_server_settings,
};
pub(super) use sse::sse_events;
pub(super) use webui::{webui_asset, webui_entrypoint};
pub use webui_assets::WebUiAssets;
