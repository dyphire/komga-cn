use komga_application::operational::AnnouncementPort;

use crate::persistence::DatabaseHandle;

use super::announcements_persistence;

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
    async fn load_announcement_read_ids(&self, user_id: &str) -> anyhow::Result<Vec<String>> {
        announcements_persistence::load_announcement_read_ids(self.db.read_pool(), user_id)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn save_announcements_read(&self, user_id: &str, ids: &[String]) -> anyhow::Result<()> {
        announcements_persistence::save_announcements_read(self.db.write_pool(), user_id, ids)
            .await
            .map_err(anyhow::Error::from)
    }
}
