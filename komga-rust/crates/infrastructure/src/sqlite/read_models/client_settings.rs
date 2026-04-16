use std::path::Path;

use serde_json::{Value, json};
use sqlx::Row;

use crate::sqlite::connect_pool;

pub async fn load_client_settings_global(
    database_file: &Path,
    allow_unauthorized_only: bool,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = if allow_unauthorized_only {
        sqlx::query(
            r#"SELECT KEY, VALUE, ALLOW_UNAUTHORIZED
             FROM CLIENT_SETTINGS_GLOBAL
             WHERE ALLOW_UNAUTHORIZED = 1
             ORDER BY KEY ASC"#,
        )
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            r#"SELECT KEY, VALUE, ALLOW_UNAUTHORIZED
             FROM CLIENT_SETTINGS_GLOBAL
             ORDER BY KEY ASC"#,
        )
        .fetch_all(&pool)
        .await?
    };

    let mut map = serde_json::Map::new();
    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<String, _>("VALUE");
        let allow_unauthorized = row.get::<bool, _>("ALLOW_UNAUTHORIZED");
        map.insert(
            key,
            json!({
                "value": value,
                "allowUnauthorized": allow_unauthorized,
            }),
        );
    }
    Ok(Value::Object(map))
}

pub async fn load_client_settings_user(
    database_file: &Path,
    user_id: &str,
) -> Result<Value, sqlx::Error> {
    let pool = connect_pool(database_file, 1).await?;
    let rows = sqlx::query(
        r#"SELECT KEY, VALUE
         FROM CLIENT_SETTINGS_USER
         WHERE USER_ID = ?
         ORDER BY KEY ASC"#,
    )
    .bind(user_id)
    .fetch_all(&pool)
    .await?;

    let mut map = serde_json::Map::new();
    for row in rows {
        let key = row.get::<String, _>("KEY");
        let value = row.get::<String, _>("VALUE");
        map.insert(key, json!({ "value": value }));
    }
    Ok(Value::Object(map))
}
