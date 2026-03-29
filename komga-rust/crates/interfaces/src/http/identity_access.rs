pub mod auth;
pub mod device_auth;

#[path = "content_auth.rs"]
mod user_session_routes;

pub(super) use user_session_routes::{
    login_set_cookie_route, logout_route, users_authentication_activity_route,
    users_by_id_authentication_activity_latest_route, users_by_id_password_route,
    users_create_route, users_delete_route, users_list_route, users_me_api_keys_create_route,
    users_me_api_keys_delete_route, users_me_api_keys_list_route,
    users_me_authentication_activity_route, users_me_password_route, users_me_route,
    users_update_route,
};
