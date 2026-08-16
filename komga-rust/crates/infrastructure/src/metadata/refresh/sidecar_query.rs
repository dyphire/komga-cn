use sqlx::{Row, SqlitePool};

pub(super) async fn load_sidecar_url_for_parent(
    pool: &SqlitePool,
    parent_url: &str,
    metadata_only: bool,
) -> anyhow::Result<Option<String>> {
    let sql = if metadata_only {
        r#"
        SELECT URL
        FROM SIDECAR
        WHERE PARENT_URL = ?
          AND LOWER(URL) LIKE '%.xml'
        ORDER BY LAST_MODIFIED_TIME DESC
        LIMIT 1
        "#
    } else {
        r#"
        SELECT URL
        FROM SIDECAR
        WHERE PARENT_URL = ?
          AND (
                LOWER(URL) LIKE '%.jpg'
             OR LOWER(URL) LIKE '%.jpeg'
             OR LOWER(URL) LIKE '%.png'
             OR LOWER(URL) LIKE '%.tbn'
             OR LOWER(URL) LIKE '%.webp'
             OR LOWER(URL) LIKE '%.gif'
             OR LOWER(URL) LIKE '%.avif'
          )
        ORDER BY LAST_MODIFIED_TIME DESC
        LIMIT 1
        "#
    };

    let row = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(parent_url)
        .fetch_optional(pool)
        .await
        .map_err(|error| {
            anyhow::anyhow!(error).context(format!("failed to load sidecar for '{parent_url}'"))
        })?;
    Ok(row.map(|row| row.get::<String, _>("URL")))
}
