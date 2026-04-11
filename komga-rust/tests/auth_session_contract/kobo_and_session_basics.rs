use super::*;

#[path = "kobo_and_session_basics/claims_and_session.rs"]
pub(crate) mod claims_and_session;
#[path = "kobo_and_session_basics/kobo_catch_all_and_ping.rs"]
mod kobo_catch_all_and_ping;
#[path = "kobo_and_session_basics/kobo_initialization_and_device_auth.rs"]
mod kobo_initialization_and_device_auth;
#[path = "kobo_and_session_basics/oauth2.rs"]
pub(crate) mod oauth2;
#[path = "kobo_and_session_basics/remember_me_and_logout.rs"]
pub(crate) mod remember_me_and_logout;
