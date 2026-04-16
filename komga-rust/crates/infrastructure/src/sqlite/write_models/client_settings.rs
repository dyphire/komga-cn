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
            r#"INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED)
               VALUES (?, ?, ?)
               ON CONFLICT(KEY) DO UPDATE
               SET VALUE = excluded.VALUE,
                   ALLOW_UNAUTHORIZED = excluded.ALLOW_UNAUTHORIZED"#,
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
            r#"INSERT INTO CLIENT_SETTINGS_USER (USER_ID, KEY, VALUE)
               VALUES (?, ?, ?)
               ON CONFLICT(USER_ID, KEY) DO UPDATE
               SET VALUE = excluded.VALUE"#,
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
        sqlx::query(r#"DELETE FROM CLIENT_SETTINGS_GLOBAL WHERE KEY = ?"#)
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
            r#"DELETE
               FROM CLIENT_SETTINGS_USER
               WHERE USER_ID = ?
               AND KEY = ?"#,
        )
        .bind(user_id)
        .bind(key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
