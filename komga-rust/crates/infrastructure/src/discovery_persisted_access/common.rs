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

pub(super) struct ScopedStringQuery<'a> {
    pub library_ids: Option<&'a [String]>,
    pub collection_id: Option<&'a str>,
    pub label: &'a str,
    pub base_sql: &'a str,
    pub collection_join: &'a str,
    pub library_column: &'a str,
    pub extra_condition: Option<&'a str>,
    pub order_by: &'a str,
}

pub(super) async fn load_persisted_scoped_strings(
    pool: &SqlitePool,
    query: &ScopedStringQuery<'_>,
) -> Result<Vec<String>, String> {
    if let Some(library_ids) = query.library_ids
        && library_ids.is_empty()
    {
        return Ok(Vec::new());
    }

    let mut builder = QueryBuilder::<Sqlite>::new(query.base_sql);
    let mut has_where = false;

    if let Some(collection_id) = query.collection_id {
        builder.push(query.collection_join);
        builder.push(" WHERE cs.COLLECTION_ID = ");
        builder.push_bind(collection_id);
        has_where = true;
    }

    if let Some(library_ids) = query.library_ids.filter(|ids| !ids.is_empty()) {
        builder.push(if has_where { " AND " } else { " WHERE " });
        builder.push(query.library_column);
        builder.push(" IN (");
        let mut separated = builder.separated(",");
        for library_id in library_ids {
            separated.push_bind(library_id);
        }
        separated.push_unseparated(")");
        has_where = true;
    }

    if let Some(extra_condition) = query.extra_condition {
        builder.push(if has_where { " AND " } else { " WHERE " });
        builder.push(extra_condition);
    }

    builder.push(" ORDER BY ");
    builder.push(query.order_by);

    let rows = builder
        .build()
        .fetch_all(pool)
        .await
        .map_err(|error| format!("query persisted {}: {error}", query.label))?;

    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("VALUE"))
        .collect())
}
