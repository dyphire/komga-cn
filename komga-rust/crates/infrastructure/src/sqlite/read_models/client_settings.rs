use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedClientGlobalSetting {
    pub(crate) key: String,
    pub(crate) value: String,
    pub(crate) allow_unauthorized: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersistedClientUserSetting {
    pub(crate) key: String,
    pub(crate) value: String,
}

pub(crate) async fn load_client_settings_global(
    pool: &SqlitePool,
    allow_unauthorized_only: bool,
) -> Result<Vec<PersistedClientGlobalSetting>, sqlx::Error> {
    let rows = if allow_unauthorized_only {
        sqlx::query(
            r#"SELECT KEY, VALUE, ALLOW_UNAUTHORIZED
             FROM CLIENT_SETTINGS_GLOBAL
             WHERE ALLOW_UNAUTHORIZED = 1
             ORDER BY KEY ASC"#,
        )
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT KEY, VALUE, ALLOW_UNAUTHORIZED
             FROM CLIENT_SETTINGS_GLOBAL
             ORDER BY KEY ASC"#,
        )
        .fetch_all(pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|row| PersistedClientGlobalSetting {
            key: row.get::<String, _>("KEY"),
            value: row.get::<String, _>("VALUE"),
            allow_unauthorized: row.get::<bool, _>("ALLOW_UNAUTHORIZED"),
        })
        .collect())
}

pub(crate) async fn load_client_settings_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<PersistedClientUserSetting>, sqlx::Error> {
    let rows = sqlx::query(
        r#"SELECT KEY, VALUE
         FROM CLIENT_SETTINGS_USER
         WHERE USER_ID = ?
         ORDER BY KEY ASC"#,
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PersistedClientUserSetting {
            key: row.get::<String, _>("KEY"),
            value: row.get::<String, _>("VALUE"),
        })
        .collect())
}
