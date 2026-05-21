use komga_application::operational::AnnouncementPort;

use crate::announcements_access;
use crate::database_handle::DatabaseHandle;

#[derive(Clone)]
pub struct AnnouncementAccess {
    db: DatabaseHandle,
}

impl AnnouncementAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl AnnouncementPort for AnnouncementAccess {
    async fn load_announcement_read_ids(&self, user_id: &str) -> Result<Vec<String>, String> {
        announcements_access::load_announcement_read_ids(self.db.read_pool(), user_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn save_announcements_read(&self, user_id: &str, ids: &[String]) -> Result<(), String> {
        announcements_access::save_announcements_read(self.db.write_pool(), user_id, ids)
            .await
            .map_err(|e| e.to_string())
    }
}
