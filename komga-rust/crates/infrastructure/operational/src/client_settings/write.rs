use komga_application::operational::{ClientGlobalSettings, ClientUserSettings};
use sqlx::SqlitePool;

pub(crate) async fn upsert_client_settings_global(
    pool: &SqlitePool,
    settings: &ClientGlobalSettings,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for (key, setting) in settings {
        sqlx::query(
            r#"INSERT INTO CLIENT_SETTINGS_GLOBAL (KEY, VALUE, ALLOW_UNAUTHORIZED)
               VALUES (?, ?, ?)
               ON CONFLICT(KEY) DO UPDATE
               SET VALUE = excluded.VALUE,
                   ALLOW_UNAUTHORIZED = excluded.ALLOW_UNAUTHORIZED"#,
        )
        .bind(key)
        .bind(&setting.value)
        .bind(setting.allow_unauthorized)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn upsert_client_settings_user(
    pool: &SqlitePool,
    user_id: &str,
    settings: &ClientUserSettings,
) -> Result<(), sqlx::Error> {
    let mut tx = pool.begin().await?;
    for (key, setting) in settings {
        sqlx::query(
            r#"INSERT INTO CLIENT_SETTINGS_USER (USER_ID, KEY, VALUE)
               VALUES (?, ?, ?)
               ON CONFLICT(USER_ID, KEY) DO UPDATE
               SET VALUE = excluded.VALUE"#,
        )
        .bind(user_id)
        .bind(key)
        .bind(&setting.value)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

pub(crate) async fn delete_client_settings_global(
    pool: &SqlitePool,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    if keys.is_empty() {
        return Ok(());
    }

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

pub(crate) async fn delete_client_settings_user(
    pool: &SqlitePool,
    user_id: &str,
    keys: &[String],
) -> Result<(), sqlx::Error> {
    if keys.is_empty() {
        return Ok(());
    }

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
