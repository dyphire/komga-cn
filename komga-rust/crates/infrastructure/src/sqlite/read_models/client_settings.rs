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
            "SELECT KEY, VALUE, ALLOW_UNAUTHORIZED\n             FROM CLIENT_SETTINGS_GLOBAL\n             WHERE ALLOW_UNAUTHORIZED = 1\n             ORDER BY KEY ASC",
        )
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query(
            "SELECT KEY, VALUE, ALLOW_UNAUTHORIZED\n             FROM CLIENT_SETTINGS_GLOBAL\n             ORDER BY KEY ASC",
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
    if !map.contains_key("webui.oauth2.hide_login") {
        map.insert(
            "webui.oauth2.hide_login".to_string(),
            json!({
                "value": "false",
                "allowUnauthorized": true,
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
        "SELECT KEY, VALUE\n         FROM CLIENT_SETTINGS_USER\n         WHERE USER_ID = ?\n         ORDER BY KEY ASC",
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
