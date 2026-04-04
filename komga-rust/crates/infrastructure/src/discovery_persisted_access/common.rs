#![allow(clippy::too_many_arguments)]

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

pub(super) async fn load_persisted_scoped_strings(
    database_file: &FsPath,
    library_ids: Option<&[String]>,
    collection_id: Option<&str>,
    label: &str,
    base_sql: &str,
    collection_join: &str,
    library_column: &str,
    extra_condition: Option<&str>,
    order_by: &str,
) -> Result<Vec<String>, String> {
    if let Some(library_ids) = library_ids
        && library_ids.is_empty()
    {
        return Ok(Vec::new());
    }

    let pool = connect_pool(database_file, 1)
        .await
        .map_err(|error| format!("open {label} db: {error}"))?;

    let mut query = QueryBuilder::<Sqlite>::new(base_sql);
    let mut has_where = false;

    if let Some(collection_id) = collection_id {
        query.push(collection_join);
        query.push(" WHERE cs.COLLECTION_ID = ");
        query.push_bind(collection_id);
        has_where = true;
    }

    if let Some(library_ids) = library_ids.filter(|ids| !ids.is_empty()) {
        query.push(if has_where { " AND " } else { " WHERE " });
        query.push(library_column);
        query.push(" IN (");
        let mut separated = query.separated(",");
        for library_id in library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(")");
        has_where = true;
    }

    if let Some(extra_condition) = extra_condition {
        query.push(if has_where { " AND " } else { " WHERE " });
        query.push(extra_condition);
    }

    query.push(" ORDER BY ");
    query.push(order_by);

    let rows = query
        .build()
        .fetch_all(&pool)
        .await
        .map_err(|error| format!("query persisted {label}: {error}"))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("VALUE"))
        .collect())
}
