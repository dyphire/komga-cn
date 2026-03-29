use crate::common_ids::{DeviceId, UserId};

use super::{AccessPrincipal, DeviceSession};

pub trait AccessPrincipalWritePort {
    fn save_principal(&self, principal: &AccessPrincipal) -> Result<(), String>;
    fn delete_principal(&self, user_id: &UserId) -> Result<(), String>;
}

pub trait DeviceSessionWritePort {
    fn save_session(&self, session: &DeviceSession) -> Result<(), String>;
    fn delete_session(&self, user_id: &UserId, device_id: &DeviceId) -> Result<(), String>;
}
