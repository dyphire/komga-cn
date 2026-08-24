use std::collections::BTreeMap;

use komga_application::operational::{
    ClientGlobalSetting, ClientGlobalSettings, ClientSettingsPort, ClientUserSetting,
    ClientUserSettings,
};
use sqlx::SqlitePool;

use komga_infrastructure_base::DatabaseHandle;
use read::{load_persisted_client_settings_global, load_persisted_client_settings_user};
pub(crate) use write::{
    delete_client_settings_global, delete_client_settings_user, upsert_client_settings_global,
    upsert_client_settings_user,
};

mod read;
mod write;

#[derive(Clone)]
pub struct ClientSettingsAccess {
    db: DatabaseHandle,
}

impl ClientSettingsAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl ClientSettingsPort for ClientSettingsAccess {
    async fn load_client_settings_global(
        &self,
        allow_unauthorized_only: bool,
    ) -> anyhow::Result<ClientGlobalSettings> {
        load_client_settings_global(self.db.read_pool(), allow_unauthorized_only)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn load_client_settings_user(&self, user_id: &str) -> anyhow::Result<ClientUserSettings> {
        load_client_settings_user(self.db.read_pool(), user_id)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn upsert_client_settings_global(
        &self,
        settings: &ClientGlobalSettings,
    ) -> anyhow::Result<()> {
        upsert_client_settings_global(self.db.write_pool(), settings)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn upsert_client_settings_user(
        &self,
        user_id: &str,
        settings: &ClientUserSettings,
    ) -> anyhow::Result<()> {
        upsert_client_settings_user(self.db.write_pool(), user_id, settings)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn delete_client_settings_global(&self, keys: &[String]) -> anyhow::Result<()> {
        delete_client_settings_global(self.db.write_pool(), keys)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn delete_client_settings_user(
        &self,
        user_id: &str,
        keys: &[String],
    ) -> anyhow::Result<()> {
        delete_client_settings_user(self.db.write_pool(), user_id, keys)
            .await
            .map_err(anyhow::Error::from)
    }
}

pub(crate) async fn load_client_settings_global(
    pool: &SqlitePool,
    allow_unauthorized_only: bool,
) -> Result<ClientGlobalSettings, sqlx::Error> {
    let rows = load_persisted_client_settings_global(pool, allow_unauthorized_only).await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.key,
                ClientGlobalSetting {
                    value: row.value,
                    allow_unauthorized: row.allow_unauthorized,
                },
            )
        })
        .collect::<BTreeMap<_, _>>())
}

pub(crate) async fn load_client_settings_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<ClientUserSettings, sqlx::Error> {
    let rows = load_persisted_client_settings_user(pool, user_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.key, ClientUserSetting { value: row.value }))
        .collect::<BTreeMap<_, _>>())
}
