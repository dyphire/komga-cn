use komga_application::operational::SyncpointPort;
use sqlx::{Row, SqlitePool};

use crate::database_handle::DatabaseHandle;

#[derive(Clone)]
pub struct SyncpointAccess {
    db: DatabaseHandle,
}

impl SyncpointAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }
}

#[async_trait::async_trait]
impl SyncpointPort for SyncpointAccess {
    async fn delete_syncpoints_by_user(&self, user_id: &str) -> anyhow::Result<()> {
        delete_syncpoints_by_user(self.db.write_pool(), user_id)
            .await
            .map_err(anyhow::Error::from)
    }

    async fn delete_syncpoints_by_user_and_key_ids(
        &self,
        user_id: &str,
        key_ids: &[String],
    ) -> anyhow::Result<()> {
        delete_syncpoints_by_user_and_key_ids(self.db.write_pool(), user_id, key_ids)
            .await
            .map_err(anyhow::Error::from)
    }
}

pub(crate) async fn delete_syncpoints_by_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    let sync_point_ids = load_syncpoint_ids_for_user(&mut tx, user_id, None).await?;
    delete_syncpoint_children(&mut tx, &sync_point_ids).await?;
    sqlx::query(
        r#"DELETE
        FROM SYNC_POINT
        WHERE USER_ID = ?"#,
    )
    .bind(user_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn delete_syncpoints_by_user_and_key_ids(
    pool: &SqlitePool,
    user_id: &str,
    key_ids: &[String],
) -> Result<(), sqlx::Error> {
    if key_ids.is_empty() {
        return delete_syncpoints_by_user(pool, user_id).await;
    }

    let mut tx = pool.begin().await?;
    let sync_point_ids = load_syncpoint_ids_for_user(&mut tx, user_id, Some(key_ids)).await?;
    delete_syncpoint_children(&mut tx, &sync_point_ids).await?;

    let mut query =
        sqlx::QueryBuilder::<sqlx::Sqlite>::new("DELETE FROM SYNC_POINT WHERE USER_ID = ");
    query.push_bind(user_id);
    query.push(" AND API_KEY_ID IN (");
    let mut separated = query.separated(", ");
    for key_id in key_ids {
        separated.push_bind(key_id);
    }
    separated.push_unseparated(")");
    query.build().execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

async fn load_syncpoint_ids_for_user(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
    key_ids: Option<&[String]>,
) -> Result<Vec<String>, sqlx::Error> {
    let mut query =
        sqlx::QueryBuilder::<sqlx::Sqlite>::new("SELECT ID FROM SYNC_POINT WHERE USER_ID = ");
    query.push_bind(user_id);
    if let Some(key_ids) = key_ids {
        query.push(" AND API_KEY_ID IN (");
        let mut separated = query.separated(", ");
        for key_id in key_ids {
            separated.push_bind(key_id);
        }
        separated.push_unseparated(")");
    }

    Ok(query
        .build()
        .fetch_all(&mut **tx)
        .await?
        .into_iter()
        .map(|row| row.get::<String, _>("ID"))
        .collect())
}

async fn delete_syncpoint_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    sync_point_ids: &[String],
) -> Result<(), sqlx::Error> {
    for sync_point_id in sync_point_ids {
        for sql in [
            "DELETE FROM SYNC_POINT_READLIST_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST_BOOK WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_READLIST WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_BOOK_REMOVED_SYNCED WHERE SYNC_POINT_ID = ?",
            "DELETE FROM SYNC_POINT_BOOK WHERE SYNC_POINT_ID = ?",
        ] {
            sqlx::query(sql)
                .bind(sync_point_id)
                .execute(&mut **tx)
                .await?;
        }
    }

    Ok(())
}
