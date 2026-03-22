use komga_domain::discovery::{AgeRestrictionKind, DiscoveryQueryContext, QueryRestrictions};
use rusqlite::types::Value as SqlValue;

pub(super) struct SqlFilters {
    pub(super) where_clause: String,
    pub(super) params: Vec<SqlValue>,
}

pub(super) fn query_filters(
    library_column: &str,
    allowed_library_ids: Option<&Vec<String>>,
    search: Option<&str>,
    search_column: Option<&str>,
    restrictions: Option<&QueryRestrictions>,
    restriction_series_alias: &str,
) -> SqlFilters {
    let mut clauses = Vec::<String>::new();
    let mut params = Vec::<SqlValue>::new();

    if let Some(allowed) = allowed_library_ids {
        let placeholders = vec!["?"; allowed.len()].join(",");
        clauses.push(format!("{library_column} IN ({placeholders})"));
        params.extend(allowed.iter().cloned().map(SqlValue::Text));
    }

    if let (Some(term), Some(column)) = (search, search_column) {
        clauses.push(format!("LOWER({column}) LIKE ?"));
        params.push(SqlValue::Text(format!("%{}%", term.to_ascii_lowercase())));
    }

    if let Some(restrictions) = restrictions {
        apply_restrictions(
            restriction_series_alias,
            restrictions,
            &mut clauses,
            &mut params,
        );
    }

    let where_clause = if clauses.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", clauses.join(" AND "))
    };

    SqlFilters {
        where_clause,
        params,
    }
}

pub(super) fn apply_restrictions(
    series_alias: &str,
    restrictions: &QueryRestrictions,
    clauses: &mut Vec<String>,
    params: &mut Vec<SqlValue>,
) {
    if !restrictions.labels_exclude.is_empty() {
        let placeholders = vec!["?"; restrictions.labels_exclude.len()].join(",");
        clauses.push(format!(
            "NOT EXISTS (SELECT 1 FROM series_labels ex WHERE ex.series_id = {series_alias}.id AND LOWER(ex.label) IN ({placeholders}))"
        ));
        params.extend(
            restrictions
                .labels_exclude
                .iter()
                .map(|label| SqlValue::Text(label.to_ascii_lowercase())),
        );
    }

    if let (Some(AgeRestrictionKind::Exclude), Some(max_age)) =
        (restrictions.age_restriction, restrictions.age)
    {
        clauses.push(format!(
            "({series_alias}.age_rating IS NULL OR {series_alias}.age_rating < ?)"
        ));
        params.push(SqlValue::Integer(max_age as i64));
    }

    if !restrictions.labels_allow.is_empty() {
        let placeholders = vec!["?"; restrictions.labels_allow.len()].join(",");
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM series_labels al WHERE al.series_id = {series_alias}.id AND LOWER(al.label) IN ({placeholders}))"
        ));
        params.extend(
            restrictions
                .labels_allow
                .iter()
                .map(|label| SqlValue::Text(label.to_ascii_lowercase())),
        );
    }
}

pub(super) fn append_in_clause(
    column: &str,
    values: &[String],
    sql: &mut String,
    params: &mut Vec<SqlValue>,
) {
    let placeholders = vec!["?"; values.len()].join(",");
    let prefix = if sql.contains(" WHERE ") {
        " AND "
    } else {
        " WHERE "
    };
    sql.push_str(prefix);
    sql.push_str(&format!("{column} IN ({placeholders})"));
    params.extend(values.iter().cloned().map(SqlValue::Text));
}

pub(super) fn append_clause(clause: &str, where_clause: &mut String) {
    if where_clause.contains(" WHERE ") {
        where_clause.push_str(" AND ");
        where_clause.push_str(clause);
    } else {
        where_clause.push_str(" WHERE ");
        where_clause.push_str(clause);
    }
}

pub(super) fn append_string_set_filter(
    column: &str,
    values: Option<&[String]>,
    where_clause: &mut String,
    params: &mut Vec<SqlValue>,
    lowercase: bool,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let placeholders = vec!["?"; values.len()].join(",");
        let lhs = if lowercase {
            format!("LOWER({column})")
        } else {
            column.to_string()
        };
        append_clause(&format!("{lhs} IN ({placeholders})"), where_clause);
        if lowercase {
            params.extend(
                values
                    .iter()
                    .map(|value| SqlValue::Text(value.to_ascii_lowercase())),
            );
        } else {
            params.extend(values.iter().cloned().map(SqlValue::Text));
        }
    }
}

pub(super) fn append_u16_set_filter(
    column: &str,
    values: Option<&[u16]>,
    where_clause: &mut String,
    params: &mut Vec<SqlValue>,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let placeholders = vec!["?"; values.len()].join(",");
        append_clause(&format!("{column} IN ({placeholders})"), where_clause);
        params.extend(values.iter().map(|value| SqlValue::Integer(*value as i64)));
    }
}

pub(super) fn append_exists_series_filter(
    table: &str,
    value_column: &str,
    values: Option<&[String]>,
    where_clause: &mut String,
    params: &mut Vec<SqlValue>,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let placeholders = vec!["?"; values.len()].join(",");
        append_clause(
            &format!(
                "EXISTS (SELECT 1 FROM {table} f WHERE f.series_id = s.id AND LOWER(f.{value_column}) IN ({placeholders}))"
            ),
            where_clause,
        );
        params.extend(
            values
                .iter()
                .map(|value| SqlValue::Text(value.to_ascii_lowercase())),
        );
    }
}

pub(super) fn effective_library_ids(
    context: &DiscoveryQueryContext,
    requested_library_ids: Option<&[String]>,
) -> Option<Vec<String>> {
    match (&context.authorized_library_ids, requested_library_ids) {
        (Some(authorized), Some(requested)) => Some(intersection(authorized, requested)),
        (Some(authorized), None) => Some(authorized.clone()),
        (None, Some(requested)) => Some(requested.to_vec()),
        (None, None) => None,
    }
}

fn intersection(authorized: &[String], requested: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|candidate| authorized.contains(*candidate))
        .cloned()
        .collect()
}
