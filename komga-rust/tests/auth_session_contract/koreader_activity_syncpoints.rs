use super::*;

#[path = "koreader_activity_syncpoints/authentication_activity.rs"]
pub(crate) mod authentication_activity;
#[path = "koreader_activity_syncpoints/koreader_api_keys.rs"]
mod koreader_api_keys;
#[path = "koreader_activity_syncpoints/remember_me_lifecycle.rs"]
pub(crate) mod remember_me_lifecycle;
#[path = "koreader_activity_syncpoints/sse_events.rs"]
mod sse_events;
#[path = "koreader_activity_syncpoints/syncpoint_deletion.rs"]
mod syncpoint_deletion;
#[path = "koreader_activity_syncpoints/user_mutation_password_demo_actuator.rs"]
pub(crate) mod user_mutation_password_demo_actuator;
