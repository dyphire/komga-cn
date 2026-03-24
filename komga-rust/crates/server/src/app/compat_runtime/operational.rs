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

pub(super) use actuator::{
    actuator_beans, actuator_health, actuator_info, actuator_logfile, actuator_metric_detail,
    actuator_metrics_index, actuator_root, actuator_shutdown, health_live, health_ready,
    metrics_text,
};
pub(super) use cors::dev_cors_middleware;
pub(super) use settings::{
    delete_syncpoints_me, delete_tasks, get_announcements, get_claim_status,
    get_client_settings_global, get_client_settings_user, get_fonts_families, get_history,
    get_oauth2_providers, get_page_hash_thumbnail, get_page_hashes, get_releases,
    get_server_settings, post_claim, post_filesystem, post_transient_books, put_announcements,
    update_server_settings,
};
pub(super) use sse::sse_events;
