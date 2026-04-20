#![allow(clippy::too_many_arguments)]

use komga_domain::common_ids::LibraryId;
use komga_domain::discovery::{AgeRestrictionKind, DiscoveryQueryContext, QueryRestrictions};
use sqlx::{QueryBuilder, Sqlite};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum SqlValue {
    Text(String),
    Integer(i64),
}

#[derive(Default)]
pub(super) struct SqlxWhereState {
    has_where: bool,
    pub(super) params: Vec<SqlValue>,
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
        builder.push(format!(r#"LOWER({column}) LIKE "#));
        let lowered = format!("%{}%", term.to_ascii_lowercase());
        builder.push_bind(lowered.clone());
        state.params.push(SqlValue::Text(lowered));
    }

    if let Some(active_restrictions) = restrictions {
        apply_restrictions_sqlx(
            restriction_series_alias,
            active_restrictions,
            builder,
            state,
        );
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
            r#"NOT EXISTS (
    SELECT 1
    FROM series_labels ex
    WHERE ex.series_id = {series_alias}.id
      AND LOWER(ex.label) IN ("#
        ));
        {
            let mut separated = builder.separated(",");
            for label in &restrictions.labels_exclude {
                let lowered = label.to_ascii_lowercase();
                separated.push_bind(lowered.clone());
                state.params.push(SqlValue::Text(lowered));
            }
            separated.push_unseparated(r#"))"#);
        }
    }

    if let (Some(AgeRestrictionKind::Exclude), Some(max_age)) =
        (restrictions.age_restriction, restrictions.age)
    {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!(
            r#"({series_alias}.age_rating IS NULL OR {series_alias}.age_rating < "#
        ));
        builder.push_bind(max_age as i64);
        builder.push(r#")"#);
        state.params.push(SqlValue::Integer(max_age as i64));
    }

    if !restrictions.labels_allow.is_empty() {
        push_sqlx_clause_prefix(builder, state);
        builder.push(format!(
            r#"EXISTS (
    SELECT 1
    FROM series_labels al
    WHERE al.series_id = {series_alias}.id
      AND LOWER(al.label) IN ("#
        ));
        {
            let mut separated = builder.separated(",");
            for label in &restrictions.labels_allow {
                let lowered = label.to_ascii_lowercase();
                separated.push_bind(lowered.clone());
                state.params.push(SqlValue::Text(lowered));
            }
            separated.push_unseparated(r#"))"#);
        }
    }
}

pub(super) fn append_in_clause_sqlx<'args>(
    column: &str,
    values: &[String],
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    push_sqlx_clause_prefix(builder, state);
    builder.push(format!(r#"{column} IN ("#));
    {
        let mut separated = builder.separated(",");
        for value in values {
            separated.push_bind(value.clone());
            state.params.push(SqlValue::Text(value.clone()));
        }
        separated.push_unseparated(r#")"#);
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
        builder.push(format!(r#"{lhs} IN ("#));
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
            separated.push_unseparated(r#")"#);
        }
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
        builder.push(format!(r#"{column} IN ("#));
        {
            let mut separated = builder.separated(",");
            for value in values {
                separated.push_bind(*value as i64);
                state.params.push(SqlValue::Integer(*value as i64));
            }
            separated.push_unseparated(r#")"#);
        }
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
            r#"EXISTS (
    SELECT 1
    FROM {table} f
    WHERE f.series_id = s.id
      AND LOWER(f.{value_column}) IN ("#
        ));
        {
            let mut separated = builder.separated(",");
            for value in values {
                let lowered = value.to_ascii_lowercase();
                separated.push_bind(lowered.clone());
                state.params.push(SqlValue::Text(lowered));
            }
            separated.push_unseparated(r#"))"#);
        }
    }
}

pub(super) fn effective_library_ids(
    context: &DiscoveryQueryContext,
    requested_library_ids: Option<&[String]>,
) -> Option<Vec<String>> {
    match (&context.authorized_library_ids, requested_library_ids) {
        (Some(authorized), Some(requested)) => Some(intersection(
            &authorized_library_strings(authorized),
            requested,
        )),
        (Some(authorized), None) => Some(authorized_library_strings(authorized)),
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

fn authorized_library_strings(authorized: &[LibraryId]) -> Vec<String> {
    authorized
        .iter()
        .map(|library_id| library_id.as_str().to_string())
        .collect()
}

fn push_sqlx_clause_prefix<'args>(
    builder: &mut QueryBuilder<'args, Sqlite>,
    state: &mut SqlxWhereState,
) {
    if state.has_where {
        builder.push(r#" AND "#);
    } else {
        builder.push(r#" WHERE "#);
        state.has_where = true;
    }
}

#[cfg(test)]
mod tests {
    use komga_domain::discovery::{AgeRestrictionKind, QueryRestrictions};
    use sqlx::{Execute, QueryBuilder, Sqlite};

    use super::{
        SqlValue, SqlxWhereState, append_clause_sqlx, append_exists_series_filter_sqlx,
        append_string_set_filter_sqlx, append_u16_set_filter_sqlx, query_filters_sqlx,
    };

    #[test]
    fn sqlx_query_filters_preserve_restriction_clause_and_bind_order() {
        let restrictions = QueryRestrictions {
            age: Some(16),
            age_restriction: Some(AgeRestrictionKind::Exclude),
            labels_allow: vec!["ALLOW-B".to_string(), "Allow-A".to_string()],
            labels_exclude: vec!["NsFw".to_string(), "BLOCKED".to_string()],
        };

        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"SELECT s.id
FROM series s"#,
        );
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
            actual_sql.ends_with(
                r#"WHERE s.library_id IN (?,?) AND LOWER(s.title) LIKE ? AND NOT EXISTS (
    SELECT 1
    FROM series_labels ex
    WHERE ex.series_id = s.id
      AND LOWER(ex.label) IN (?,?)) AND (s.age_rating IS NULL OR s.age_rating < ?) AND EXISTS (
    SELECT 1
    FROM series_labels al
    WHERE al.series_id = s.id
      AND LOWER(al.label) IN (?,?))"#
            ),
            "unexpected sql: {actual_sql}",
        );

        assert_eq!(
            state.params,
            vec![
                SqlValue::Text("lib-2".to_string()),
                SqlValue::Text("lib-1".to_string()),
                SqlValue::Text("%mixed%".to_string()),
                SqlValue::Text("nsfw".to_string()),
                SqlValue::Text("blocked".to_string()),
                SqlValue::Integer(16),
                SqlValue::Text("allow-b".to_string()),
                SqlValue::Text("allow-a".to_string()),
            ]
        );
    }

    #[test]
    fn sqlx_extended_predicates_preserve_exists_lowercase_and_parameter_order() {
        let mut builder = QueryBuilder::<Sqlite>::new(
            r#"SELECT s.id
FROM series s"#,
        );
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
        append_u16_set_filter_sqlx("s.age_rating", Some(&[10, 16]), &mut builder, &mut state);

        let query = builder.build();
        assert!(query.sql().ends_with(
            r#"WHERE s.deleted = 0 AND LOWER(s.read_status) IN (?,?) AND EXISTS (
    SELECT 1
    FROM series_genres f
    WHERE f.series_id = s.id
      AND LOWER(f.genre) IN (?,?)) AND s.release_date IN (?) AND s.age_rating IN (?,?)"#
        ));

        assert_eq!(
            state.params,
            vec![
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
