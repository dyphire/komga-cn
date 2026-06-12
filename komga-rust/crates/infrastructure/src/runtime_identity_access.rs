mod access;
mod user_mutation;

pub use crate::auth::runtime_identity_access::{
    invalidate_user_sessions, persisted_update_password_by_user_id,
};
pub use access::IdentityAccess;
