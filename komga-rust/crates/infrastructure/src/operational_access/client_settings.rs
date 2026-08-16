use std::collections::BTreeMap;

use komga_application::operational::{
    ClientGlobalSetting, ClientGlobalSettings, ClientSettingsPort, ClientUserSetting,
    ClientUserSettings,
};
use sqlx::SqlitePool;

use crate::database_handle::DatabaseHandle;
use crate::sqlite::read_models::client_settings::{
    load_client_settings_global as load_client_settings_global_model,
    load_client_settings_user as load_client_settings_user_model,
};
use crate::sqlite::write_models::client_settings::{
    delete_client_settings_global as delete_client_settings_global_model,
    delete_client_settings_user as delete_client_settings_user_model,
    upsert_client_settings_global as upsert_client_settings_global_model,
    upsert_client_settings_user as upsert_client_settings_user_model,
};

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
    let rows = load_client_settings_global_model(pool, allow_unauthorized_only).await?;
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
    let rows = load_client_settings_user_model(pool, user_id).await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.key, ClientUserSetting { value: row.value }))
        .collect::<BTreeMap<_, _>>())
}

pub(crate) async fn upsert_client_settings_global(
    pool: &SqlitePool,
    settings: &ClientGlobalSettings,
) -> Result<(), sqlx::Error> {
    upsert_client_settings_global_model(pool, settings).await
}

pub(crate) async fn upsert_client_settings_user(
    pool: &SqlitePool,
    user_id: &str,
    settings: &ClientUserSettings,
) -> Result<(), sqlx::Error> {
    upsert_client_settings_user_model(pool, user_id, settings).await
}

pub(crate) async fn delete_client_settings_global(
    pool: &SqlitePool,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    delete_client_settings_global_model(pool, keys).await
}

pub(crate) async fn delete_client_settings_user(
    pool: &SqlitePool,
    user_id: &str,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    delete_client_settings_user_model(pool, user_id, keys).await
}
