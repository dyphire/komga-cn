use crate::common_ids::{DeviceId, LibraryId, UserId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccessRole {
    Admin,
    User,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryAccessRule {
    pub library_id: LibraryId,
    pub can_read: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccessPrincipal {
    pub user_id: UserId,
    pub role: AccessRole,
    pub library_rules: Vec<LibraryAccessRule>,
    pub age_restriction: Option<u16>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceSession {
    pub user_id: UserId,
    pub device_id: DeviceId,
    pub token_hash: String,
}
