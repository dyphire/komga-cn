use komga_domain::discovery::{AgeRestrictionKind, DiscoveryQueryContext, QueryRestrictions};
use sqlx::{QueryBuilder, Sqlite};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SqlValue {
    Text(String),
    Integer(i64),
}

pub(super) struct SqlFilters {
    pub(super) where_clause: String,
    pub(super) params: Vec<SqlValue>,
}

#[derive(Default)]
pub(super) struct SqlxWhereState {
    has_where: bool,
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

pub(super) fn query_filters_sqlx<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    library_column: &str,
    allowed_library_ids: Option<&Vec<String>>,
    search: Option<&str>,
    search_column: Option<&str>,
    restrictions: Option<&QueryRestrictions>,
    restriction_series_alias: &str,
) {
    if let Some(allowed) = allowed_library_ids {
        append_in_clause_sqlx(library_column, allowed, builder, state);
    }

    if let (Some(term), Some(column)) = (search, search_column) {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!("LOWER({column}) LIKE "));
        let lowered = format!("%{}%", term.to_ascii_lowercase());
        builder.push_bind(lowered.clone());
        state.params.push(SqlValue::Text(lowered));
    }

    if let Some(active_restrictions) = restrictions {
        apply_restrictions_sqlx(restriction_series_alias, active_restrictions, builder, state);
    }
}

pub(super) fn apply_restrictions_sqlx<'args>(
    series_alias: &str,
    restrictions: &QueryRestrictions,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    if !restrictions.labels_exclude.is_empty() {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!(
            "NOT EXISTS (SELECT 1 FROM series_labels ex WHERE ex.series_id = {series_alias}.id AND LOWER(ex.label) IN ("
        ));
        {
            let mut separated = builder.separated(",");
            for label in &restrictions.labels_exclude {
                let lowered = label.to_ascii_lowercase();
                separated.push_bind(lowered.clone());
                state.params.push(SqlValue::Text(lowered));
            }
            separated.push_unseparated("))");
        }
    }

    if let (Some(AgeRestrictionKind::Exclude), Some(max_age)) =
        (restrictions.age_restriction, restrictions.age)
    {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!("({series_alias}.age_rating IS NULL OR {series_alias}.age_rating < "));
        builder.push_bind(max_age as i64);
        builder.push(")");
        state.params.push(SqlValue::Integer(max_age as i64));
    }

    if !restrictions.labels_allow.is_empty() {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!(
            "EXISTS (SELECT 1 FROM series_labels al WHERE al.series_id = {series_alias}.id AND LOWER(al.label) IN ("
        ));
        {
            let mut separated = builder.separated(",");
            for label in &restrictions.labels_allow {
                let lowered = label.to_ascii_lowercase();
                separated.push_bind(lowered.clone());
                state.params.push(SqlValue::Text(lowered));
            }
            separated.push_unseparated("))");
        }
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

pub(super) fn append_in_clause_sqlx<'args>(
    column: &str,
    values: &[String],
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    push_sqlx_clause_prefix(builder, state);
    builder.push(format!("{column} IN ("));
    {
        let mut separated = builder.separated(",");
        for value in values {
            separated.push_bind(value.clone());
            state.params.push(SqlValue::Text(value.clone()));
        }
        separated.push_unseparated(")");
    }
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

pub(super) fn append_clause_sqlx<'args>(
    clause: &str,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    push_sqlx_clause_prefix(builder, state);
    builder.push(clause);
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

pub(super) fn append_string_set_filter_sqlx<'args>(
    column: &str,
    values: Option<&[String]>,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
    lowercase: bool,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        let lhs = if lowercase {
            format!("LOWER({column})")
        } else {
            column.to_string()
        };
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!("{lhs} IN ("));
        {
            let mut separated = builder.separated(",");
            for value in values {
                if lowercase {
                    let lowered = value.to_ascii_lowercase();
                    separated.push_bind(lowered.clone());
                    state.params.push(SqlValue::Text(lowered));
                } else {
                    separated.push_bind(value.clone());
                    state.params.push(SqlValue::Text(value.clone()));
                }
            }
            separated.push_unseparated(")");
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

pub(super) fn append_u16_set_filter_sqlx<'args>(
    column: &str,
    values: Option<&[u16]>,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!("{column} IN ("));
        {
            let mut separated = builder.separated(",");
            for value in values {
                separated.push_bind(*value as i64);
                state.params.push(SqlValue::Integer(*value as i64));
            }
            separated.push_unseparated(")");
        }
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

pub(super) fn append_exists_series_filter_sqlx<'args>(
    table: &str,
    value_column: &str,
    values: Option<&[String]>,
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    if let Some(values) = values
        && !values.is_empty()
    {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!(
            "EXISTS (SELECT 1 FROM {table} f WHERE f.series_id = s.id AND LOWER(f.{value_column}) IN ("
        ));
        {
            let mut separated = builder.separated(",");
            for value in values {
                let lowered = value.to_ascii_lowercase();
                separated.push_bind(lowered.clone());
                state.params.push(SqlValue::Text(lowered));
            }
            separated.push_unseparated("))");
        }
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

fn push_sqlx_clause_prefix<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    if state.has_where {
        builder.push(" AND ");
    } else {
        builder.push(" WHERE ");
        state.has_where = true;
    }
}

#[cfg(test)]
mod tests {
    use komga_domain::discovery::{AgeRestrictionKind, QueryRestrictions};
    use sqlx::{Execute, QueryBuilder, Sqlite};

    use super::{
        SqlxWhereState, append_clause, append_clause_sqlx, append_exists_series_filter,
        append_exists_series_filter_sqlx, append_string_set_filter, append_string_set_filter_sqlx,
        append_u16_set_filter, append_u16_set_filter_sqlx, query_filters, query_filters_sqlx,
        SqlValue,
    };

    #[test]
    fn sqlx_query_filters_preserve_restriction_clause_and_bind_order() {
        let restrictions = QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec!["ALLOW-B".to_string(), "Allow-A".to_string()],
            labels_exclude: vec!["NsFw".to_string(), "BLOCKED".to_string()],
        };

        let expected = query_filters(
            "s.library_id",
            Some(&vec!["lib-2".to_string(), "lib-1".to_string()]),
            Some("MiXeD"),
            Some("s.title"),
            Some(&restrictions),
            "s",
        );

        let mut builder = QueryBuilder::<Sqlite>::new("SELECT s.id FROM series s");
        let mut state = SqlxWhereState::default();
        query_filters_sqlx(
            &mut builder,
            &mut state,
            "s.library_id",
            Some(&vec!["lib-2".to_string(), "lib-1".to_string()]),
            Some("MiXeD"),
            Some("s.title"),
            Some(&restrictions),
            "s",
        );

        let query = builder.build();
        let actual_sql = query.sql();

        assert!(
            actual_sql.ends_with(expected.where_clause.as_str()),
            "expected SQL to end with legacy where clause\nexpected suffix: {}\nactual: {}",
            expected.where_clause,
            actual_sql,
        );

        assert_eq!(state.params, expected.params);
    }

    #[test]
    fn sqlx_extended_predicates_preserve_exists_lowercase_and_parameter_order() {
        let mut expected_where = String::new();
        let mut expected_params = Vec::<SqlValue>::new();

        append_clause("s.deleted = ?", &mut expected_where);
        expected_params.push(SqlValue::Integer(0));
        append_string_set_filter(
            "s.read_status",
            Some(&["READ".to_string(), "UNREAD".to_string()]),
            &mut expected_where,
            &mut expected_params,
            true,
        );
        append_exists_series_filter(
            "series_genres",
            "genre",
            Some(&["Fantasy".to_string(), "Drama".to_string()]),
            &mut expected_where,
            &mut expected_params,
        );
        append_string_set_filter(
            "s.release_date",
            Some(&["2024-01-01".to_string()]),
            &mut expected_where,
            &mut expected_params,
            false,
        );
        append_u16_set_filter(
            "s.age_rating",
            Some(&[10, 16]),
            &mut expected_where,
            &mut expected_params,
        );

        let mut builder = QueryBuilder::<Sqlite>::new("SELECT s.id FROM series s");
        let mut state = SqlxWhereState::default();
        append_clause_sqlx("s.deleted = 0", &mut builder, &mut state);
        append_string_set_filter_sqlx(
            "s.read_status",
            Some(&["READ".to_string(), "UNREAD".to_string()]),
            &mut builder,
            &mut state,
            true,
        );
        append_exists_series_filter_sqlx(
            "series_genres",
            "genre",
            Some(&["Fantasy".to_string(), "Drama".to_string()]),
            &mut builder,
            &mut state,
        );
        append_string_set_filter_sqlx(
            "s.release_date",
            Some(&["2024-01-01".to_string()]),
            &mut builder,
            &mut state,
            false,
        );
        append_u16_set_filter_sqlx(
            "s.age_rating",
            Some(&[10, 16]),
            &mut builder,
            &mut state,
        );

        let query = builder.build();
        assert!(
            query.sql().ends_with(
                " WHERE s.deleted = 0 AND LOWER(s.read_status) IN (?,?) AND EXISTS (SELECT 1 FROM series_genres f WHERE f.series_id = s.id AND LOWER(f.genre) IN (?,?)) AND s.release_date IN (?) AND s.age_rating IN (?,?)"
            )
        );

        let mut expected_params_for_sqlx = vec![SqlValue::Text("read".to_string()), SqlValue::Text("unread".to_string())];
        expected_params_for_sqlx.extend(vec![
            SqlValue::Text("fantasy".to_string()),
            SqlValue::Text("drama".to_string()),
            SqlValue::Text("2024-01-01".to_string()),
            SqlValue::Integer(10),
            SqlValue::Integer(16),
        ]);
        assert_eq!(state.params, expected_params_for_sqlx);

        assert!(expected_where.contains("LOWER(s.read_status) IN (?,?)"));
        assert!(
            expected_where.contains(
                "EXISTS (SELECT 1 FROM series_genres f WHERE f.series_id = s.id AND LOWER(f.genre) IN (?,?))"
            )
        );
        assert_eq!(
            expected_params,
            vec![
                SqlValue::Integer(0),
                SqlValue::Text("read".to_string()),
                SqlValue::Text("unread".to_string()),
                SqlValue::Text("fantasy".to_string()),
                SqlValue::Text("drama".to_string()),
                SqlValue::Text("2024-01-01".to_string()),
                SqlValue::Integer(10),
                SqlValue::Integer(16),
            ]
        );
    }
}
