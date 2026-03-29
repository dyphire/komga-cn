use super::*;

pub(super) fn parse_csv_values(raw: &str) -> Vec<String> {
    if raw.trim().is_empty() {
        return vec![];
    }
    raw.split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) async fn load_persisted_library_strings(
    database_file: &FsPath,
    library_id: Option<&str>,
    label: &str,
    sql: &str,
    sql_all: &str,
) -> Result<Vec<String>, String> {
    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open {label} db: {error}"))?;

    let rows = if let Some(library_id) = library_id {
        sqlx::query(sql).bind(library_id).fetch_all(&pool).await
    } else {
        sqlx::query(sql_all).fetch_all(&pool).await
    }
    .map_err(|error| format!("query persisted {label}: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("VALUE"))
        .collect())
}
