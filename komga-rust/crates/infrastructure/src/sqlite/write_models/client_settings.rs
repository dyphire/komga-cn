use std::path::Path;

use crate::sqlite::connect_pool;

pub async fn upsert_client_settings_global(
    database_file: &Path,
    settings: &[(String, String, bool)],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for (key, value, allow_unauthorized) in settings {
        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED)\n             VALUES (?, ?, ?)\n             ON CONFLICT(KEY) DO UPDATE\n             SET VALUE = excluded.VALUE,\n                 ALLOW_UNAUTHORIZED = excluded.ALLOW_UNAUTHORIZED",
        )
        .bind(key)
        .bind(value)
        .bind(*allow_unauthorized)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn upsert_client_settings_user(
    database_file: &Path,
    user_id: &str,
    settings: &[(String, String)],
) -> Result<(), sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for (key, value) in settings {
        sqlx::query(
            "INSERT INTO CLIENT_SETTINGS_USER (USER_ID, KEY, VALUE)\n             VALUES (?, ?, ?)\n             ON CONFLICT(USER_ID, KEY) DO UPDATE\n             SET VALUE = excluded.VALUE",
        )
        .bind(user_id)
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn delete_client_settings_global(
    database_file: &Path,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    if keys.is_empty() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for key in keys {
        sqlx::query("DELETE FROM CLIENT_SETTINGS_GLOBAL WHERE KEY = ?")
            .bind(key)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub async fn delete_client_settings_user(
    database_file: &Path,
    user_id: &str,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    if keys.is_empty() {
        return Ok(());
    }

    let pool = connect_pool(database_file, 1).await?;
    let mut tx = pool.begin().await?;
    for key in keys {
        sqlx::query(
            "DELETE\n             FROM CLIENT_SETTINGS_USER\n             WHERE USER_ID = ?\n             AND KEY = ?",
        )
        .bind(user_id)
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
