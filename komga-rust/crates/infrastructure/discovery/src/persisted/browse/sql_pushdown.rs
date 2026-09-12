//! SQL pushdown for persisted browse queries (optimization items 3 & 8).
//!
//! Strategy: translate the in-memory browse engine's condition and sort semantics
//! into a SQL `WHERE` / `ORDER BY` page-ID query, then load only the page's rows
//! by ID (two-stage). Whenever a condition or sort mode cannot be translated
//! faithfully (ICU collator sorts, regex, poster conditions, search relevance),
//! the caller falls back to the existing in-memory engine path, so behavior is
//! bit-identical to the previous implementation.

use std::collections::HashMap;

use komga_application::discovery::{collect_book_release_date_offsets, collect_series_release_date_offsets};
use komga_domain::discovery::{
    AgeRatingCondition, AgeRestrictionKind, BookCondition, BookPosterCondition,
    BookValueCondition, DateCondition, FilterOperator, InclusionCondition, MediaProfile,
    MediaStatus, NumberCondition, ReadStatus, ReadStatusCondition, SeriesCondition,
    SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
use sqlx::{Row, Sqlite, SqlitePool};

use super::models::{
    BooksFilterCriteria, PersistedBooksBrowseQuery, PersistedBooksSortMode,
    PersistedSeriesBrowseQuery, PersistedSeriesSortMode,
};
use super::{DiscoveryQueryContext, SqliteDiscoveryBrowseService};

/// A page of series/books IDs plus the total element count.
pub(super) struct SqlPage {
    pub(super) ids: Vec<String>,
    pub(super) total_elements: usize,
}

#[derive(Clone)]
enum SqlBind {
    Text(String),
    Int(i64),
    Real(f64),
}

#[derive(Default)]
struct SqlExpr {
    sql: String,
    binds: Vec<SqlBind>,
}

impl SqlExpr {
    fn push(&mut self, text: &str) {
        self.sql.push_str(text);
    }

    fn pushs(&mut self, parts: &[&str]) {
        for part in parts {
            self.sql.push_str(part);
        }
    }

    fn bind_text(&mut self, value: &str) {
        self.sql.push('?');
        self.binds.push(SqlBind::Text(value.to_string()));
    }

    fn bind_int(&mut self, value: i64) {
        self.sql.push('?');
        self.binds.push(SqlBind::Int(value));
    }

    fn bind_real(&mut self, value: f64) {
        self.sql.push('?');
        self.binds.push(SqlBind::Real(value));
    }

    fn absorb(&mut self, mut other: SqlExpr) {
        self.sql.push_str(&other.sql);
        self.binds.append(&mut other.binds);
    }

    fn constant(value: &str) -> Self {
        Self {
            sql: value.to_string(),
            binds: Vec::new(),
        }
    }
}

/// `ORDER BY` content plus the joins it requires.
struct OrderSpec {
    sql: String,
    /// Bind a user id into `READ_PROGRESS_SERIES` when a series read-date sort needs it.
    rps_user: Option<String>,
    /// Bind a collection id into `COLLECTION_SERIES` when a collection-number sort needs it.
    collection_id: Option<String>,
    /// Bind a user id into `READ_PROGRESS` (books) when a read-progress sort needs it.
    rp_user: Option<String>,
    /// Bind a readlist id into `READLIST_BOOK` when a readlist-number sort needs it.
    readlist_id: Option<String>,
}

fn first_collection_id(query: &PersistedSeriesBrowseQuery) -> Option<&str> {
    fn visit(condition: &SeriesCondition) -> Option<&str> {
        match condition {
            SeriesCondition::Value(SeriesValueCondition::CollectionId(
                InclusionCondition::Include(values),
            )) => values.first().map(|value| value.as_str()),
            SeriesCondition::Composite(composite) => composite.conditions.iter().find_map(visit),
            _ => None,
        }
    }

    query
        .filters
        .collection_ids
        .as_ref()
        .and_then(|ids| ids.first().map(String::as_str))
        .or_else(|| query.condition.as_ref().and_then(visit))
}

fn first_readlist_id(query: &PersistedBooksBrowseQuery) -> Option<&str> {
    fn visit(condition: &BookCondition) -> Option<&str> {
        match condition {
            BookCondition::Value(BookValueCondition::ReadListId(InclusionCondition::Include(
                values,
            ))) => values.first().map(|value| value.as_str()),
            BookCondition::Composite(composite) => composite.conditions.iter().find_map(visit),
            _ => None,
        }
    }

    query.condition.as_ref().and_then(visit)
}

/// Escape a LIKE literal (engine treats filter values as plain substrings).
fn escape_like(value: &str) -> String {
    value.replace('!', "!!").replace('%', "!%").replace('_', "!_")
}

/// `col LIKE <prefix>?<suffix> ESCAPE '!'` OR-ed across values. `col` must be
/// the full expression (callers pass `LOWER(...)` when they want it).
fn push_contains_patterns(expr: &mut SqlExpr, col: &str, values: &[String], prefix: &str, suffix: &str) {
    if values.is_empty() {
        expr.push("1=0");
        return;
    }
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            expr.push(" OR ");
        }
        expr.pushs(&[col, " LIKE "]);
        if !prefix.is_empty() {
            expr.pushs(&[prefix, " || "]);
        }
        expr.bind_text(&escape_like(value));
        if !suffix.is_empty() {
            expr.pushs(&[" || ", suffix]);
        }
        expr.push(" ESCAPE '!'");
    }
}

fn push_text_in_list<S: AsRef<str>>(expr: &mut SqlExpr, col: &str, values: &[S], lower: bool) {
    expr.push(col);
    expr.push(" IN (");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            expr.push(", ");
        }
        if lower {
            expr.bind_text(&value.as_ref().to_ascii_lowercase());
        } else {
            expr.bind_text(value.as_ref());
        }
    }
    expr.push(")");
}

fn push_int_in_list(expr: &mut SqlExpr, col: &str, values: &[u16]) {
    expr.push(col);
    expr.push(" IN (");
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            expr.push(", ");
        }
        expr.bind_int(i64::from(*value));
    }
    expr.push(")");
}

fn string_single_expr(col: &str, condition: &StringCondition) -> Option<SqlExpr> {
    let mut e = SqlExpr::default();
    match condition {
        StringCondition::Exact(InclusionCondition::Include(values)) => {
            if values.is_empty() {
                e.push("1=0");
            } else {
                push_text_in_list(&mut e, col, values, false);
            }
        }
        StringCondition::Exact(InclusionCondition::Exclude(values)) => {
            if values.is_empty() {
                e.push("1=1");
            } else {
                e.push("NOT ");
                push_text_in_list(&mut e, col, values, false);
            }
        }
        StringCondition::Contains(InclusionCondition::Include(values)) => {
            push_contains_patterns(&mut e, col, values, "'%'", "'%'");
        }
        StringCondition::Contains(InclusionCondition::Exclude(values)) => {
            e.push("NOT (");
            push_contains_patterns(&mut e, col, values, "'%'", "'%'");
            e.push(")");
        }
        StringCondition::StartsWith(InclusionCondition::Include(values)) => {
            push_contains_patterns(&mut e, col, values, "", "'%'");
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(values)) => {
            e.push("NOT (");
            push_contains_patterns(&mut e, col, values, "", "'%'");
            e.push(")");
        }
        StringCondition::EndsWith(InclusionCondition::Include(values)) => {
            push_contains_patterns(&mut e, col, values, "'%'", "");
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(values)) => {
            e.push("NOT (");
            push_contains_patterns(&mut e, col, values, "'%'", "");
            e.push(")");
        }
        StringCondition::Regex(patterns) => {
            if patterns.is_empty() {
                e.push("1=0");
            } else if patterns.iter().any(|p| !compilable_regex(p)) {
                return None;
            } else {
                e.push("(");
                for (index, pattern) in patterns.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    e.pushs(&["LOWER(", col, ") REGEXP ('(?i)' || ?)"]);
                    e.bind_text(pattern);
                }
                e.push(")");
            }
        }
        StringCondition::IsEmpty => e.pushs(&["(", col, " = '')"]),
        StringCondition::IsNotEmpty => e.pushs(&["(", col, " != '')"]),
    }
    Some(e)
}

/// Poster condition over the THUMBNAIL_BOOK child table via EXISTS.
///
/// Semantics mirror `book_condition::matches_poster_condition`: Include = any poster
/// row matches any condition, Exclude = no poster row matches (books without any
/// poster pass Exclude). A single poster matches when the type (if specified) and
/// the selected flag (if specified) both hold.
fn book_poster_expr(condition: &InclusionCondition<BookPosterCondition>) -> Option<SqlExpr> {
    let mut e = SqlExpr::default();
    match condition {
        InclusionCondition::Include(conditions) => {
            if conditions.is_empty() {
                e.push("1=0");
            } else {
                e.push("EXISTS (SELECT 1 FROM THUMBNAIL_BOOK tb WHERE tb.BOOK_ID = b.ID AND ");
                push_poster_matches(&mut e, conditions);
                e.push(")");
            }
        }
        InclusionCondition::Exclude(conditions) => {
            if conditions.is_empty() {
                e.push("1=1");
            } else {
                e.push("NOT EXISTS (SELECT 1 FROM THUMBNAIL_BOOK tb WHERE tb.BOOK_ID = b.ID AND ");
                push_poster_matches(&mut e, conditions);
                e.push(")");
            }
        }
    }
    Some(e)
}

fn push_poster_matches(e: &mut SqlExpr, conditions: &[BookPosterCondition]) {
    e.push("(");
    for (index, condition) in conditions.iter().enumerate() {
        if index > 0 {
            e.push(" OR ");
        }
        e.push("(");
        let mut any = false;
        if let Some(thumbnail_type) = condition.thumbnail_type {
            e.pushs(&["tb.TYPE = "]);
            e.bind_text(thumbnail_type.persisted_name());
            any = true;
        }
        if let Some(selected) = condition.selected {
            if any {
                e.push(" AND ");
            }
            e.pushs(&["tb.SELECTED = "]);
            e.bind_int(if selected { 1 } else { 0 });
            any = true;
        }
        if !any {
            e.push("1=0");
        }
        e.push(")");
    }
    e.push(")");
}

/// Multi-valued string condition over a child table via EXISTS.
fn string_multi_exists(
    table: &str,
    join_col: &str,
    parent_id: &str,
    col: &str,
    condition: &StringCondition,
) -> Option<SqlExpr> {
    let mut inner = SqlExpr::default();
    let mut negate = false;
    match condition {
        StringCondition::Exact(InclusionCondition::Include(values)) => {
            if values.is_empty() {
                return Some(SqlExpr::constant("1=0"));
            }
            push_text_in_list(&mut inner, &format!("LOWER({col})"), values, false);
        }
        StringCondition::Exact(InclusionCondition::Exclude(values)) => {
            if values.is_empty() {
                return Some(SqlExpr::constant("1=1"));
            }
            negate = true;
            push_text_in_list(&mut inner, &format!("LOWER({col})"), values, false);
        }
        StringCondition::Contains(InclusionCondition::Include(values)) => {
            push_contains_patterns(&mut inner, &format!("LOWER({col})"), values, "'%'", "'%'");
        }
        StringCondition::Contains(InclusionCondition::Exclude(values)) => {
            negate = true;
            push_contains_patterns(&mut inner, &format!("LOWER({col})"), values, "'%'", "'%'");
        }
        StringCondition::StartsWith(InclusionCondition::Include(values)) => {
            push_contains_patterns(&mut inner, &format!("LOWER({col})"), values, "", "'%'");
        }
        StringCondition::StartsWith(InclusionCondition::Exclude(values)) => {
            negate = true;
            push_contains_patterns(&mut inner, &format!("LOWER({col})"), values, "", "'%'");
        }
        StringCondition::EndsWith(InclusionCondition::Include(values)) => {
            push_contains_patterns(&mut inner, &format!("LOWER({col})"), values, "'%'", "");
        }
        StringCondition::EndsWith(InclusionCondition::Exclude(values)) => {
            negate = true;
            push_contains_patterns(&mut inner, &format!("LOWER({col})"), values, "'%'", "");
        }
        StringCondition::Regex(patterns) => {
            if patterns.is_empty() {
                inner.push("1=0");
            } else if patterns.iter().any(|p| !compilable_regex(p)) {
                return None;
            } else {
                inner.push("(");
                for (index, pattern) in patterns.iter().enumerate() {
                    if index > 0 {
                        inner.push(" OR ");
                    }
                    inner.pushs(&["LOWER(", col, ") REGEXP ('(?i)' || ?)"]);
                    inner.bind_text(pattern);
                }
                inner.push(")");
            }
        }
        StringCondition::IsEmpty => {
            return Some(SqlExpr::constant(&format!(
                "NOT EXISTS (SELECT 1 FROM {table} WHERE {join_col} = {parent_id})"
            )));
        }
        StringCondition::IsNotEmpty => {
            return Some(SqlExpr::constant(&format!(
                "EXISTS (SELECT 1 FROM {table} WHERE {join_col} = {parent_id})"
            )));
        }
    }
    let mut e = SqlExpr::default();
    e.pushs(&[
        if negate { "NOT EXISTS" } else { "EXISTS" },
        " (SELECT 1 FROM ",
        table,
        " WHERE ",
        join_col,
        " = ",
        parent_id,
        " AND ",
    ]);
    e.absorb(inner);
    e.push(")");
    Some(e)
}

fn date_condition_expr(
    col: &str,
    condition: &DateCondition,
    cutoffs: &HashMap<i64, Option<String>>,
    no_cutoff_result: bool,
) -> Option<SqlExpr> {
    let mut e = SqlExpr::default();
    match condition {
        DateCondition::Exact(InclusionCondition::Include(values)) => {
            if values.is_empty() {
                e.push("1=0");
            } else {
                push_text_in_list(&mut e, col, values, false);
            }
        }
        DateCondition::Exact(InclusionCondition::Exclude(values)) => {
            if values.is_empty() {
                e.push("1=1");
            } else {
                e.push("(");
                e.push(col);
                e.push(" IS NULL OR ");
                e.push("NOT ");
                push_text_in_list(&mut e, col, values, false);
                e.push(")");
            }
        }
        DateCondition::Before(value) => {
            e.pushs(&["(", col, " IS NOT NULL AND ", col, " < "]);
            e.bind_text(value);
            e.push(")");
        }
        DateCondition::After(value) => {
            e.pushs(&["(", col, " IS NOT NULL AND ", col, " > "]);
            e.bind_text(value);
            e.push(")");
        }
        DateCondition::Contains(InclusionCondition::Include(values)) => {
            e.pushs(&["(", col, " IS NOT NULL AND ("]);
            push_contains_patterns(&mut e, &format!("LOWER({col})"), values, "'%'", "'%'");
            e.push("))");
        }
        DateCondition::Contains(InclusionCondition::Exclude(values)) => {
            e.push("(");
            e.push(col);
            e.push(" IS NULL OR NOT (");
            push_contains_patterns(&mut e, &format!("LOWER({col})"), values, "'%'", "'%'");
            e.push("))");
        }
        DateCondition::StartsWith(InclusionCondition::Include(values)) => {
            e.pushs(&["(", col, " IS NOT NULL AND ("]);
            push_contains_patterns(&mut e, &format!("LOWER({col})"), values, "", "'%'");
            e.push("))");
        }
        DateCondition::StartsWith(InclusionCondition::Exclude(values)) => {
            e.push("(");
            e.push(col);
            e.push(" IS NULL OR NOT (");
            push_contains_patterns(&mut e, &format!("LOWER({col})"), values, "", "'%'");
            e.push("))");
        }
        DateCondition::EndsWith(InclusionCondition::Include(values)) => {
            e.pushs(&["(", col, " IS NOT NULL AND ("]);
            push_contains_patterns(&mut e, &format!("LOWER({col})"), values, "'%'", "");
            e.push("))");
        }
        DateCondition::EndsWith(InclusionCondition::Exclude(values)) => {
            e.push("(");
            e.push(col);
            e.push(" IS NULL OR NOT (");
            push_contains_patterns(&mut e, &format!("LOWER({col})"), values, "'%'", "");
            e.push("))");
        }
        DateCondition::WithinLastDays(days) => match cutoffs.get(days).and_then(Option::as_deref) {
            Some(cutoff) => {
                e.pushs(&["(", col, " IS NOT NULL AND ", col, " > "]);
                e.bind_text(cutoff);
                e.push(")");
            }
            None => {
                if no_cutoff_result {
                    e.push("1=1");
                } else {
                    e.push("1=0");
                }
            }
        },
        DateCondition::OutsideLastDays(days) => match cutoffs.get(days).and_then(Option::as_deref) {
            Some(cutoff) => {
                e.pushs(&["(", col, " IS NOT NULL AND ", col, " < "]);
                e.bind_text(cutoff);
                e.push(")");
            }
            None => {
                if no_cutoff_result {
                    e.push("1=1");
                } else {
                    e.push("1=0");
                }
            }
        },
        DateCondition::IsEmpty => e.pushs(&["(", col, " IS NULL)"]),
        DateCondition::IsNotEmpty => e.pushs(&["(", col, " IS NOT NULL)"]),
    }
    Some(e)
}

fn age_rating_series_expr(condition: &AgeRatingCondition) -> SqlExpr {
    let mut e = SqlExpr::default();
    match condition {
        AgeRatingCondition::Exact(InclusionCondition::Include(values)) => {
            if values.is_empty() {
                e.push("1=0");
            } else {
                push_int_in_list(&mut e, "sm.AGE_RATING", values);
            }
        }
        AgeRatingCondition::Exact(InclusionCondition::Exclude(values)) => {
            if values.is_empty() {
                e.push("1=1");
            } else {
                e.push("(sm.AGE_RATING IS NULL OR ");
                e.push("NOT ");
                push_int_in_list(&mut e, "sm.AGE_RATING", values);
                e.push(")");
            }
        }
        AgeRatingCondition::ExactOrEmpty(values) => {
            if values.is_empty() {
                e.push("sm.AGE_RATING IS NULL");
            } else {
                e.push("(sm.AGE_RATING IS NULL OR ");
                push_int_in_list(&mut e, "sm.AGE_RATING", values);
                e.push(")");
            }
        }
        AgeRatingCondition::GreaterThan(value) => {
            e.pushs(&["(sm.AGE_RATING IS NOT NULL AND sm.AGE_RATING > "]);
            e.bind_int(i64::from(*value));
            e.push(")");
        }
        AgeRatingCondition::LessThan(value) => {
            e.pushs(&["(sm.AGE_RATING IS NOT NULL AND sm.AGE_RATING < "]);
            e.bind_int(i64::from(*value));
            e.push(")");
        }
        AgeRatingCondition::IsEmpty => e.push("sm.AGE_RATING IS NULL"),
        AgeRatingCondition::IsNotEmpty => e.push("sm.AGE_RATING IS NOT NULL"),
    }
    e
}

fn age_rating_inclusion_expr(condition: &InclusionCondition<u16>) -> SqlExpr {
    let mut e = SqlExpr::default();
    match condition {
        InclusionCondition::Include(values) => {
            if values.is_empty() {
                e.push("1=0");
            } else {
                push_int_in_list(&mut e, "sm.AGE_RATING", values);
            }
        }
        InclusionCondition::Exclude(values) => {
            if values.is_empty() {
                e.push("1=1");
            } else {
                e.push("(sm.AGE_RATING IS NULL OR ");
                e.push("NOT ");
                push_int_in_list(&mut e, "sm.AGE_RATING", values);
                e.push(")");
            }
        }
    }
    e
}

/// Engine: Include -> value present && any ignore-case match; Exclude -> present && NOT any (missing -> true).
fn optional_text_inclusion_expr(col: &str, condition: &InclusionCondition<String>) -> SqlExpr {
    let mut e = SqlExpr::default();
    match condition {
        InclusionCondition::Include(values) => {
            if values.is_empty() {
                e.push("1=0");
            } else {
                e.pushs(&["(", col, " IS NOT NULL AND LOWER(", col, ") IN ("]);
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        e.push(", ");
                    }
                    e.bind_text(&value.to_ascii_lowercase());
                }
                e.push("))");
            }
        }
        InclusionCondition::Exclude(values) => {
            if values.is_empty() {
                e.push("1=1");
            } else {
                e.push("(");
                e.push(col);
                e.push(" IS NULL OR LOWER(");
                e.push(col);
                e.push(") NOT IN (");
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        e.push(", ");
                    }
                    e.bind_text(&value.to_ascii_lowercase());
                }
                e.push("))");
            }
        }
    }
    e
}

/// Engine: `values` matching against `author` = "NAME::ROLE" (or NAME) lowercased.
fn series_author_expr(condition: &StringCondition) -> Option<SqlExpr> {
    let author_col = "LOWER(bmaa.NAME || '::' || COALESCE(bmaa.ROLE, ''))";
    let mut e = SqlExpr::default();
    match condition {
        StringCondition::Contains(InclusionCondition::Include(values)) => {
            e.push("EXISTS (SELECT 1 FROM BOOK_METADATA_AGGREGATION_AUTHOR bmaa WHERE bmaa.SERIES_ID = s.ID AND ");
            push_contains_patterns(&mut e, author_col, values, "'%'", "'%'");
            e.push(")");
            Some(e)
        }
        StringCondition::Contains(InclusionCondition::Exclude(values)) => {
            e.push("NOT EXISTS (SELECT 1 FROM BOOK_METADATA_AGGREGATION_AUTHOR bmaa WHERE bmaa.SERIES_ID = s.ID AND ");
            push_contains_patterns(&mut e, author_col, values, "'%'", "'%'");
            e.push(")");
            Some(e)
        }
        StringCondition::IsEmpty => {
            e.push("NOT EXISTS (SELECT 1 FROM BOOK_METADATA_AGGREGATION_AUTHOR bmaa WHERE bmaa.SERIES_ID = s.ID)");
            Some(e)
        }
        StringCondition::IsNotEmpty => {
            e.push("EXISTS (SELECT 1 FROM BOOK_METADATA_AGGREGATION_AUTHOR bmaa WHERE bmaa.SERIES_ID = s.ID)");
            Some(e)
        }
        StringCondition::StartsWith(_) | StringCondition::EndsWith(_) | StringCondition::Regex(_) => {
            Some(SqlExpr::constant("1=0"))
        }
        StringCondition::Exact(_) => {
            author_exact_exists("BOOK_METADATA_AGGREGATION_AUTHOR", "bmaa", "bmaa.SERIES_ID = s.ID", condition)
        }
    }
}


/// Exact author matching, mirroring `author_matches_filter` semantics:
/// a value without `::`/`,` matches any role for that name; `name::role`
/// (or `name,role`) matches both components; `::role` matches an empty name
/// plus the role. Include = any author matches any value; Exclude = no author
/// matches any value.
fn author_exact_exists(table: &str, alias: &str, parent: &str, condition: &StringCondition) -> Option<SqlExpr> {
    let (values, negate) = match condition {
        StringCondition::Exact(InclusionCondition::Include(values)) => (values, false),
        StringCondition::Exact(InclusionCondition::Exclude(values)) => (values, true),
        _ => return None,
    };
    if values.is_empty() {
        return Some(SqlExpr::constant(if negate { "1=1" } else { "1=0" }));
    }
    let mut e = SqlExpr::default();
    e.push(&format!(
        "{}EXISTS (SELECT 1 FROM {table} {alias} WHERE {parent} AND (",
        if negate { "NOT " } else { "" },
    ));
    for (index, value) in values.iter().enumerate() {
        if index > 0 {
            e.push(" OR ");
        }
        if value.contains("::") || value.contains(',') {
            let (name, role) = if let Some(role) = value.strip_prefix("::") {
                ("", role)
            } else if let Some((n, r)) = value.split_once("::").or_else(|| value.split_once(',')) {
                (n, r)
            } else {
                (value.as_str(), "")
            };
            e.push(&format!(
                "(LOWER(COALESCE({alias}.NAME, '')) = ",
                alias = alias,
            ));
            e.bind_text(name);
            e.push(&format!(" AND LOWER(COALESCE({alias}.ROLE, '')) = ", alias = alias));
            e.bind_text(role);
            e.push(")");
        } else {
            e.push(&format!("LOWER({alias}.NAME) = ", alias = alias));
            e.bind_text(value);
        }
    }
    e.push("))");
    Some(e)
}

fn book_author_expr(condition: &StringCondition) -> Option<SqlExpr> {
    let author_col = "LOWER(ba.NAME || '::' || COALESCE(ba.ROLE, ''))";
    let mut e = SqlExpr::default();
    match condition {
        StringCondition::Contains(InclusionCondition::Include(values)) => {
            e.push("EXISTS (SELECT 1 FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID AND ");
            push_contains_patterns(&mut e, author_col, values, "'%'", "'%'");
            e.push(")");
            Some(e)
        }
        StringCondition::Contains(InclusionCondition::Exclude(values)) => {
            e.push("NOT EXISTS (SELECT 1 FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID AND ");
            push_contains_patterns(&mut e, author_col, values, "'%'", "'%'");
            e.push(")");
            Some(e)
        }
        StringCondition::IsEmpty => {
            e.push("NOT EXISTS (SELECT 1 FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID)");
            Some(e)
        }
        StringCondition::IsNotEmpty => {
            e.push("EXISTS (SELECT 1 FROM BOOK_METADATA_AUTHOR ba WHERE ba.BOOK_ID = b.ID)");
            Some(e)
        }
        StringCondition::StartsWith(_) | StringCondition::EndsWith(_) | StringCondition::Regex(_) => {
            Some(SqlExpr::constant("1=0"))
        }
        StringCondition::Exact(_) => {
            author_exact_exists("BOOK_METADATA_AUTHOR", "ba", "ba.BOOK_ID = b.ID", condition)
        }
    }
}

fn series_tag_expr(condition: &StringCondition) -> Option<SqlExpr> {
    match condition {
        StringCondition::IsEmpty => Some(SqlExpr {
            sql: "NOT EXISTS (SELECT 1 FROM SERIES_METADATA_TAG smt WHERE smt.SERIES_ID = s.ID) AND NOT EXISTS (SELECT 1 FROM BOOK_METADATA_AGGREGATION_TAG bmat WHERE bmat.SERIES_ID = s.ID)"
                .to_string(),
            binds: Vec::new(),
        }),
        StringCondition::IsNotEmpty => Some(SqlExpr {
            sql: "EXISTS (SELECT 1 FROM SERIES_METADATA_TAG smt WHERE smt.SERIES_ID = s.ID) OR EXISTS (SELECT 1 FROM BOOK_METADATA_AGGREGATION_TAG bmat WHERE bmat.SERIES_ID = s.ID)"
                .to_string(),
            binds: Vec::new(),
        }),
        _ => {
            // Exclude semantics must negate the OR of positive matches
            // (NOT (smt_match OR bmat_match)), not OR the negations
            // ((NOT smt_match) OR (NOT bmat_match)): a series carrying a
            // matching series-level tag must fail Exclude even without any
            // book-level tag. Mirror the engine's `!any(...)` check.
            let (negate, inner) = match condition {
                StringCondition::Exact(InclusionCondition::Exclude(values)) => (
                    true,
                    StringCondition::Exact(InclusionCondition::Include(values.clone())),
                ),
                StringCondition::Contains(InclusionCondition::Exclude(values)) => (
                    true,
                    StringCondition::Contains(InclusionCondition::Include(values.clone())),
                ),
                StringCondition::StartsWith(InclusionCondition::Exclude(values)) => (
                    true,
                    StringCondition::StartsWith(InclusionCondition::Include(values.clone())),
                ),
                StringCondition::EndsWith(InclusionCondition::Exclude(values)) => (
                    true,
                    StringCondition::EndsWith(InclusionCondition::Include(values.clone())),
                ),
                other => (false, other.clone()),
            };
            let smt = string_multi_exists("SERIES_METADATA_TAG", "SERIES_ID", "s.ID", "TAG", &inner)?;
            let bmat = string_multi_exists("BOOK_METADATA_AGGREGATION_TAG", "SERIES_ID", "s.ID", "TAG", &inner)?;
            let mut e = SqlExpr::default();
            e.push(if negate { "NOT (" } else { "(" });
            e.absorb(smt);
            e.push(" OR ");
            e.absorb(bmat);
            e.push(")");
            Some(e)
        }
    }
}

fn series_read_status_expr(condition: &ReadStatusCondition, user_id: Option<&str>) -> Option<SqlExpr> {
    let Some(user_id) = user_id else {
        return Some(SqlExpr::constant("1=0"));
    };
    let mut e = SqlExpr::default();
    let status_pred = |e: &mut SqlExpr, status: ReadStatus| match status {
        ReadStatus::Unread => {
            e.push("NOT EXISTS (SELECT 1 FROM READ_PROGRESS_SERIES rps WHERE rps.SERIES_ID = s.ID AND rps.USER_ID = ");
            e.bind_text(user_id);
            e.push(")");
        }
        ReadStatus::Read => {
            e.push("EXISTS (SELECT 1 FROM READ_PROGRESS_SERIES rps WHERE rps.SERIES_ID = s.ID AND rps.USER_ID = ");
            e.bind_text(user_id);
            e.push(" AND MAX(rps.READ_COUNT, 0) = s.BOOK_COUNT)");
        }
        ReadStatus::InProgress => {
            e.push("EXISTS (SELECT 1 FROM READ_PROGRESS_SERIES rps WHERE rps.SERIES_ID = s.ID AND rps.USER_ID = ");
            e.bind_text(user_id);
            e.push(" AND MAX(rps.READ_COUNT, 0) != s.BOOK_COUNT)");
        }
    };
    match condition {
        ReadStatusCondition::Include(statuses) => {
            if statuses.is_empty() {
                e.push("1=0");
            } else {
                for (index, status) in statuses.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    status_pred(&mut e, *status);
                }
            }
        }
        ReadStatusCondition::Exclude(statuses) => {
            if statuses.is_empty() {
                e.push("1=1");
            } else {
                e.push("NOT (");
                for (index, status) in statuses.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    status_pred(&mut e, *status);
                }
                e.push(")");
            }
        }
    }
    Some(e)
}

fn book_read_status_expr(condition: &ReadStatusCondition, user_id: Option<&str>) -> Option<SqlExpr> {
    let Some(user_id) = user_id else {
        return Some(SqlExpr::constant("1=0"));
    };
    let mut e = SqlExpr::default();
    let status_pred = |e: &mut SqlExpr, status: ReadStatus| match status {
        ReadStatus::Unread => {
            e.push("NOT EXISTS (SELECT 1 FROM READ_PROGRESS rp2 WHERE rp2.BOOK_ID = b.ID AND rp2.USER_ID = ");
            e.bind_text(user_id);
            e.push(")");
        }
        ReadStatus::Read => {
            e.push("EXISTS (SELECT 1 FROM READ_PROGRESS rp2 WHERE rp2.BOOK_ID = b.ID AND rp2.USER_ID = ");
            e.bind_text(user_id);
            e.push(" AND rp2.COMPLETED = 1)");
        }
        ReadStatus::InProgress => {
            e.push("EXISTS (SELECT 1 FROM READ_PROGRESS rp2 WHERE rp2.BOOK_ID = b.ID AND rp2.USER_ID = ");
            e.bind_text(user_id);
            e.push(" AND (rp2.COMPLETED IS NULL OR rp2.COMPLETED = 0))");
        }
    };
    match condition {
        ReadStatusCondition::Include(statuses) => {
            if statuses.is_empty() {
                e.push("1=0");
            } else {
                for (index, status) in statuses.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    status_pred(&mut e, *status);
                }
            }
        }
        ReadStatusCondition::Exclude(statuses) => {
            if statuses.is_empty() {
                e.push("1=1");
            } else {
                e.push("NOT (");
                for (index, status) in statuses.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    status_pred(&mut e, *status);
                }
                e.push(")");
            }
        }
    }
    Some(e)
}

fn media_profile_types(profile: MediaProfile) -> &'static [&'static str] {
    match profile {
        MediaProfile::Divina => &[
            "application/zip",
            "application/x-rar-compressed",
            "application/x-rar-compressed; version=4",
            "application/x-rar-compressed; version=5",
        ],
        MediaProfile::Epub => &["application/epub+zip", "application/x-mobipocket-ebook"],
        MediaProfile::Pdf => &["application/pdf"],
    }
}

fn media_profile_expr(condition: &InclusionCondition<MediaProfile>) -> SqlExpr {
    let mut e = SqlExpr::default();
    match condition {
        InclusionCondition::Include(profiles) => {
            if profiles.is_empty() {
                e.push("1=0");
            } else {
                for (index, profile) in profiles.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    e.push("COALESCE(m.MEDIA_TYPE, '') IN (");
                    for (type_index, media_type) in media_profile_types(*profile).iter().enumerate() {
                        if type_index > 0 {
                            e.push(", ");
                        }
                        e.bind_text(media_type);
                    }
                    e.push(")");
                }
            }
        }
        InclusionCondition::Exclude(profiles) => {
            if profiles.is_empty() {
                e.push("1=1");
            } else {
                e.push("NOT (");
                for (index, profile) in profiles.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    e.push("COALESCE(m.MEDIA_TYPE, '') IN (");
                    for (type_index, media_type) in media_profile_types(*profile).iter().enumerate() {
                        if type_index > 0 {
                            e.push(", ");
                        }
                        e.bind_text(media_type);
                    }
                    e.push(")");
                }
                e.push(")");
            }
        }
    }
    e
}

fn media_status_expr(condition: &InclusionCondition<MediaStatus>) -> SqlExpr {
    let mut e = SqlExpr::default();
    let push_statuses = |e: &mut SqlExpr, statuses: &[MediaStatus]| {
        for (index, status) in statuses.iter().enumerate() {
            if index > 0 {
                e.push(", ");
            }
            e.bind_text(status.persisted_name());
        }
    };
    match condition {
        InclusionCondition::Include(statuses) => {
            if statuses.is_empty() {
                e.push("1=0");
            } else {
                e.push("COALESCE(m.STATUS, 'UNKNOWN') IN (");
                push_statuses(&mut e, statuses);
                e.push(")");
            }
        }
        InclusionCondition::Exclude(statuses) => {
            if statuses.is_empty() {
                e.push("1=1");
            } else {
                e.push("COALESCE(m.STATUS, 'UNKNOWN') NOT IN (");
                push_statuses(&mut e, statuses);
                e.push(")");
            }
        }
    }
    e
}

fn number_sort_expr(condition: &NumberCondition) -> Option<SqlExpr> {
    let mut e = SqlExpr::default();
    match condition {
        NumberCondition::Exact(InclusionCondition::Include(values)) => {
            let parsed: Vec<f64> = values.iter().filter_map(|v| v.parse::<f64>().ok()).collect();
            if parsed.is_empty() {
                e.push("1=0");
            } else {
                for (index, expected) in parsed.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    e.push("ABS(COALESCE(bm.NUMBER_SORT, 0) - ");
                    e.bind_real(*expected);
                    e.push(") <= 2.220446049250313e-16");
                }
            }
        }
        NumberCondition::Exact(InclusionCondition::Exclude(values)) => {
            let parsed: Vec<f64> = values.iter().filter_map(|v| v.parse::<f64>().ok()).collect();
            if parsed.is_empty() {
                e.push("1=1");
            } else {
                e.push("NOT (");
                for (index, expected) in parsed.iter().enumerate() {
                    if index > 0 {
                        e.push(" OR ");
                    }
                    e.push("ABS(COALESCE(bm.NUMBER_SORT, 0) - ");
                    e.bind_real(*expected);
                    e.push(") <= 2.220446049250313e-16");
                }
                e.push(")");
            }
        }
        NumberCondition::GreaterThan(value) => match value.parse::<f64>() {
            Ok(threshold) => {
                e.push("COALESCE(bm.NUMBER_SORT, 0) > ");
                e.bind_real(threshold);
            }
            Err(_) => e.push("1=0"),
        },
        NumberCondition::LessThan(value) => match value.parse::<f64>() {
            Ok(threshold) => {
                e.push("COALESCE(bm.NUMBER_SORT, 0) < ");
                e.bind_real(threshold);
            }
            Err(_) => e.push("1=0"),
        },
    }
    Some(e)
}

fn translate_series_condition(
    condition: &SeriesCondition,
    user_id: Option<&str>,
    cutoffs: &HashMap<i64, Option<String>>,
) -> Option<SqlExpr> {
    match condition {
        SeriesCondition::Value(value) => {
            let mut e = SqlExpr::default();
            match value {
                SeriesValueCondition::LibraryId(inc) => {
                    if empty_include(inc) {
                        e.push("1=0");
                    } else if empty_exclude(inc) {
                        e.push("1=1");
                    } else {
                        match inc {
                            InclusionCondition::Include(ids) => {
                                push_text_in_list(&mut e, "LOWER(s.LIBRARY_ID)", ids, true);
                            }
                            InclusionCondition::Exclude(ids) => {
                                e.push("NOT ");
                                push_text_in_list(&mut e, "LOWER(s.LIBRARY_ID)", ids, true);
                            }
                        }
                    }
                }
                SeriesValueCondition::CollectionId(inc) => {
                    if empty_include(inc) {
                        e.push("1=0");
                    } else if empty_exclude(inc) {
                        e.push("1=1");
                    } else {
                        match inc {
                            InclusionCondition::Include(ids) => {
                                e.push("EXISTS (SELECT 1 FROM COLLECTION_SERIES cs WHERE cs.SERIES_ID = s.ID AND cs.COLLECTION_ID IN (");
                                for (index, id) in ids.iter().enumerate() {
                                    if index > 0 {
                                        e.push(", ");
                                    }
                                    e.bind_text(id.as_str());
                                }
                                e.push("))");
                            }
                            InclusionCondition::Exclude(ids) => {
                                e.push("NOT EXISTS (SELECT 1 FROM COLLECTION_SERIES cs WHERE cs.SERIES_ID = s.ID AND cs.COLLECTION_ID IN (");
                                for (index, id) in ids.iter().enumerate() {
                                    if index > 0 {
                                        e.push(", ");
                                    }
                                    e.bind_text(id.as_str());
                                }
                                e.push("))");
                            }
                        }
                    }
                }
                SeriesValueCondition::Title(condition) => {
                    let inner = string_single_expr("LOWER(COALESCE(sm.TITLE, s.NAME))", condition)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::TitleSort(condition) => {
                    let inner = string_single_expr(
                        "LOWER(COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME))",
                        condition,
                    )?;
                    e.absorb(inner);
                }
                SeriesValueCondition::Deleted(value) => {
                    e.push(if *value { "s.DELETED_DATE IS NOT NULL" } else { "s.DELETED_DATE IS NULL" });
                }
                SeriesValueCondition::OneShot(value) => {
                    e.push(if *value { "s.ONESHOT != 0" } else { "s.ONESHOT = 0" });
                }
                SeriesValueCondition::ReadStatus(condition) => {
                    let inner = series_read_status_expr(condition, user_id)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::Genre(condition) => {
                    let inner = string_multi_exists("SERIES_METADATA_GENRE", "SERIES_ID", "s.ID", "GENRE", condition)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::Tag(condition) => {
                    let inner = series_tag_expr(condition)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::Language(inc) => {
                    let inner = optional_text_inclusion_expr("sm.LANGUAGE", inc);
                    e.absorb(inner);
                }
                SeriesValueCondition::Publisher(inc) => {
                    let inner = optional_text_inclusion_expr("sm.PUBLISHER", inc);
                    e.absorb(inner);
                }
                SeriesValueCondition::AgeRating(condition) => {
                    let inner = age_rating_series_expr(condition);
                    e.absorb(inner);
                }
                SeriesValueCondition::ReleaseDate(condition) => {
                    let inner = date_condition_expr("bma.RELEASE_DATE", condition, cutoffs, true)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::SharingLabel(condition) => {
                    let inner = string_multi_exists("SERIES_METADATA_SHARING", "SERIES_ID", "s.ID", "LABEL", condition)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Include(statuses)) => {
                    if statuses.is_empty() {
                        e.push("1=0");
                    } else {
                        e.push("COALESCE(sm.STATUS, 'ONGOING') IN (");
                        for (index, status) in statuses.iter().enumerate() {
                            if index > 0 {
                                e.push(", ");
                            }
                            e.bind_text(status.persisted_name());
                        }
                        e.push(")");
                    }
                }
                SeriesValueCondition::SeriesStatus(SeriesStatusCondition::Exclude(statuses)) => {
                    if statuses.is_empty() {
                        e.push("1=1");
                    } else {
                        e.push("COALESCE(sm.STATUS, 'ONGOING') NOT IN (");
                        for (index, status) in statuses.iter().enumerate() {
                            if index > 0 {
                                e.push(", ");
                            }
                            e.bind_text(status.persisted_name());
                        }
                        e.push(")");
                    }
                }
                SeriesValueCondition::Complete(value) => {
                    if *value {
                        e.push("(sm.TOTAL_BOOK_COUNT IS NOT NULL AND sm.TOTAL_BOOK_COUNT = s.BOOK_COUNT)");
                    } else {
                        e.push("(sm.TOTAL_BOOK_COUNT IS NOT NULL AND sm.TOTAL_BOOK_COUNT != s.BOOK_COUNT)");
                    }
                }
                SeriesValueCondition::Author(condition) => {
                    let inner = series_author_expr(condition)?;
                    e.absorb(inner);
                }
                SeriesValueCondition::ExcludeNewlyAdded(value) => {
                    if *value {
                        e.push("s.CREATED_DATE != s.LAST_MODIFIED_DATE");
                    } else {
                        e.push("1=1");
                    }
                }
            }
            Some(e)
        }
        SeriesCondition::Composite(composite) => composite_expr(
            composite.operator,
            &composite.conditions,
            |child| translate_series_condition(child, user_id, cutoffs),
        ),
    }
}

fn translate_book_condition(
    condition: &BookCondition,
    user_id: Option<&str>,
    cutoffs: &HashMap<i64, Option<String>>,
) -> Option<SqlExpr> {
    match condition {
        BookCondition::Value(value) => {
            let mut e = SqlExpr::default();
            match value {
                BookValueCondition::LibraryId(inc) => {
                    if empty_include(inc) {
                        e.push("1=0");
                    } else if empty_exclude(inc) {
                        e.push("1=1");
                    } else {
                        match inc {
                            InclusionCondition::Include(ids) => {
                                push_text_in_list(&mut e, "LOWER(b.LIBRARY_ID)", ids, true);
                            }
                            InclusionCondition::Exclude(ids) => {
                                e.push("NOT ");
                                push_text_in_list(&mut e, "LOWER(b.LIBRARY_ID)", ids, true);
                            }
                        }
                    }
                }
                BookValueCondition::SeriesId(inc) => {
                    if empty_include(inc) {
                        e.push("1=0");
                    } else if empty_exclude(inc) {
                        e.push("1=1");
                    } else {
                        match inc {
                            InclusionCondition::Include(ids) => {
                                push_text_in_list(&mut e, "LOWER(b.SERIES_ID)", ids, true);
                            }
                            InclusionCondition::Exclude(ids) => {
                                e.push("NOT ");
                                push_text_in_list(&mut e, "LOWER(b.SERIES_ID)", ids, true);
                            }
                        }
                    }
                }
                BookValueCondition::ReadListId(inc) => {
                    if empty_include(inc) {
                        e.push("1=0");
                    } else if empty_exclude(inc) {
                        e.push("1=1");
                    } else {
                        match inc {
                            InclusionCondition::Include(ids) => {
                                e.push("EXISTS (SELECT 1 FROM READLIST_BOOK rb2 WHERE rb2.BOOK_ID = b.ID AND rb2.READLIST_ID IN (");
                                for (index, id) in ids.iter().enumerate() {
                                    if index > 0 {
                                        e.push(", ");
                                    }
                                    e.bind_text(id.as_str());
                                }
                                e.push("))");
                            }
                            InclusionCondition::Exclude(ids) => {
                                e.push("NOT EXISTS (SELECT 1 FROM READLIST_BOOK rb2 WHERE rb2.BOOK_ID = b.ID AND rb2.READLIST_ID IN (");
                                for (index, id) in ids.iter().enumerate() {
                                    if index > 0 {
                                        e.push(", ");
                                    }
                                    e.bind_text(id.as_str());
                                }
                                e.push("))");
                            }
                        }
                    }
                }
                BookValueCondition::Title(condition) => {
                    let inner = string_single_expr("LOWER(COALESCE(bm.TITLE, b.NAME))", condition)?;
                    e.absorb(inner);
                }
                BookValueCondition::Deleted(value) => {
                    e.push(if *value { "b.DELETED_DATE IS NOT NULL" } else { "b.DELETED_DATE IS NULL" });
                }
                BookValueCondition::OneShot(value) => {
                    e.push(if *value { "s.ONESHOT != 0" } else { "s.ONESHOT = 0" });
                }
                BookValueCondition::Tag(condition) => {
                    let inner = string_multi_exists("BOOK_METADATA_TAG", "BOOK_ID", "b.ID", "TAG", condition)?;
                    e.absorb(inner);
                }
                BookValueCondition::Genre(condition) => {
                    let inner = string_multi_exists("SERIES_METADATA_GENRE", "SERIES_ID", "s.ID", "GENRE", condition)?;
                    e.absorb(inner);
                }
                BookValueCondition::Language(inc) => {
                    let inner = optional_text_inclusion_expr("sm.LANGUAGE", inc);
                    e.absorb(inner);
                }
                BookValueCondition::Publisher(inc) => {
                    let inner = optional_text_inclusion_expr("sm.PUBLISHER", inc);
                    e.absorb(inner);
                }
                BookValueCondition::AgeRating(inc) => {
                    let inner = age_rating_inclusion_expr(inc);
                    e.absorb(inner);
                }
                BookValueCondition::ReadStatus(condition) => {
                    let inner = book_read_status_expr(condition, user_id)?;
                    e.absorb(inner);
                }
                BookValueCondition::MediaProfile(inc) => {
                    let inner = media_profile_expr(inc);
                    e.absorb(inner);
                }
                BookValueCondition::MediaStatus(inc) => {
                    let inner = media_status_expr(inc);
                    e.absorb(inner);
                }
                BookValueCondition::Author(condition) => {
                    let inner = book_author_expr(condition)?;
                    e.absorb(inner);
                }
                BookValueCondition::Poster(inc) => {
                    let inner = book_poster_expr(inc)?;
                    e.absorb(inner);
                }
                BookValueCondition::NumberSort(condition) => {
                    let inner = number_sort_expr(condition)?;
                    e.absorb(inner);
                }
                BookValueCondition::ReleaseDate(condition) => {
                    let inner = date_condition_expr("bm.RELEASE_DATE", condition, cutoffs, false)?;
                    e.absorb(inner);
                }
            }
            Some(e)
        }
        BookCondition::Composite(composite) => composite_expr(
            composite.operator,
            &composite.conditions,
            |child| translate_book_condition(child, user_id, cutoffs),
        ),
    }
}

fn composite_expr<T>(
    operator: FilterOperator,
    conditions: &[T],
    translate: impl Fn(&T) -> Option<SqlExpr>,
) -> Option<SqlExpr> {
    let children: Option<Vec<SqlExpr>> = conditions.iter().map(&translate).collect();
    let children = children?;
    if children.is_empty() {
        return Some(SqlExpr::constant("1=1"));
    }
    let separator = match operator {
        FilterOperator::All => " AND ",
        FilterOperator::Any => " OR ",
    };
    let mut e = SqlExpr::default();
    if children.len() == 1 {
        e.absorb(children.into_iter().next().expect("len checked"));
    } else {
        e.push("(");
        for (index, child) in children.into_iter().enumerate() {
            if index > 0 {
                e.push(separator);
            }
            e.absorb(child);
        }
        e.push(")");
    }
    Some(e)
}

fn empty_include<T>(inc: &InclusionCondition<T>) -> bool {
    matches!(inc, InclusionCondition::Include(values) if values.is_empty())
}

fn empty_exclude<T>(inc: &InclusionCondition<T>) -> bool {
    matches!(inc, InclusionCondition::Exclude(values) if values.is_empty())
}

fn series_order_by(query: &PersistedSeriesBrowseQuery, user_id: Option<&str>) -> Option<OrderSpec> {
    let mut order = OrderSpec {
        sql: String::new(),
        rps_user: None,
        collection_id: None,
        rp_user: None,
        readlist_id: None,
    };
    if query.sort_modes.iter().any(|m| {
        matches!(m, PersistedSeriesSortMode::ReadDateAsc | PersistedSeriesSortMode::ReadDateDesc)
    }) {
        order.rps_user = user_id.map(|id| id.to_string());
    }
    if query.sort_modes.iter().any(|m| {
        matches!(
            m,
            PersistedSeriesSortMode::CollectionNumberAsc | PersistedSeriesSortMode::CollectionNumberDesc
        )
    }) {
        order.collection_id = first_collection_id(query).map(|id| id.to_string());
    }
    if query.sort_modes.is_empty() {
        order.sql = "s.ID ASC".to_string();
        return Some(order);
    }
    for mode in &query.sort_modes {
        let key = match mode {
            PersistedSeriesSortMode::TitleAsc => {
                "COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE icu_names ASC".to_string()
            }
            PersistedSeriesSortMode::TitleDesc => {
                "COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE icu_names DESC".to_string()
            }
            PersistedSeriesSortMode::NameAsc => "s.NAME COLLATE icu_names ASC".to_string(),
            PersistedSeriesSortMode::NameDesc => "s.NAME COLLATE icu_names DESC".to_string(),
            PersistedSeriesSortMode::RelevanceAsc | PersistedSeriesSortMode::RelevanceDesc => return None,
            PersistedSeriesSortMode::Random => "RANDOM()".to_string(),
            PersistedSeriesSortMode::CreatedAsc => "s.CREATED_DATE ASC".to_string(),
            PersistedSeriesSortMode::CreatedDesc => "s.CREATED_DATE DESC".to_string(),
            PersistedSeriesSortMode::LastModifiedAsc => "s.LAST_MODIFIED_DATE ASC".to_string(),
            PersistedSeriesSortMode::LastModifiedDesc => "s.LAST_MODIFIED_DATE DESC".to_string(),
            PersistedSeriesSortMode::ReadDateAsc | PersistedSeriesSortMode::ReadDateDesc => {
                if order.rps_user.is_some() {
                    let direction = if matches!(mode, PersistedSeriesSortMode::ReadDateAsc) {
                        "ASC"
                    } else {
                        "DESC"
                    };
                    format!("rps.MOST_RECENT_READ_DATE {direction}")
                } else {
                    "NULL".to_string()
                }
            }
            PersistedSeriesSortMode::CollectionNumberAsc | PersistedSeriesSortMode::CollectionNumberDesc => {
                if order.collection_id.is_some() {
                    let direction =
                        if matches!(mode, PersistedSeriesSortMode::CollectionNumberAsc) {
                            "ASC"
                        } else {
                            "DESC"
                        };
                    format!("cs.NUMBER {direction}")
                } else {
                    "NULL".to_string()
                }
            }
            PersistedSeriesSortMode::ReleaseDateAsc => "bma.RELEASE_DATE ASC".to_string(),
            PersistedSeriesSortMode::ReleaseDateDesc => "bma.RELEASE_DATE DESC".to_string(),
            PersistedSeriesSortMode::BooksCountAsc => "s.BOOK_COUNT ASC".to_string(),
            PersistedSeriesSortMode::BooksCountDesc => "s.BOOK_COUNT DESC".to_string(),
        };
        if !order.sql.is_empty() {
            order.sql.push_str(", ");
        }
        order.sql.push_str(&key);
    }
    if !order.sql.is_empty() {
        order.sql.push_str(", ");
    }
    order.sql.push_str("s.ID ASC");
    Some(order)
}

fn books_order_by(query: &PersistedBooksBrowseQuery, user_id: Option<&str>) -> Option<OrderSpec> {
    let mut order = OrderSpec {
        sql: String::new(),
        rps_user: None,
        collection_id: None,
        rp_user: None,
        readlist_id: None,
    };
    if query.sort_modes.iter().any(|m| {
        matches!(
            m,
            PersistedBooksSortMode::ReadProgressLastModifiedDateAsc
                | PersistedBooksSortMode::ReadProgressLastModifiedDateDesc
                | PersistedBooksSortMode::ReadProgressReadDateAsc
                | PersistedBooksSortMode::ReadProgressReadDateDesc
        )
    }) {
        order.rp_user = user_id.map(|id| id.to_string());
    }
    if query.sort_modes.iter().any(|m| {
        matches!(
            m,
            PersistedBooksSortMode::ReadListNumberAsc | PersistedBooksSortMode::ReadListNumberDesc
        )
    }) {
        order.readlist_id = first_readlist_id(query).map(|id| id.to_string());
    }
    if query.sort_modes.is_empty() {
        // Empty sort semantics: stable ID order (byte order == Rust String Ord).
        order.sql = "b.ID ASC".to_string();
        return Some(order);
    }
    let fallback_desc = query.sort_modes.last().map(|m| matches!(m, PersistedBooksSortMode::TitleDesc
            | PersistedBooksSortMode::NameDesc
            | PersistedBooksSortMode::SeriesTitleDesc
            | PersistedBooksSortMode::CreatedDateDesc
            | PersistedBooksSortMode::LastModifiedDateDesc
            | PersistedBooksSortMode::FileSizeDesc
            | PersistedBooksSortMode::FileHashDesc
            | PersistedBooksSortMode::UrlDesc
            | PersistedBooksSortMode::MediaStatusDesc
            | PersistedBooksSortMode::MediaCommentDesc
            | PersistedBooksSortMode::MediaTypeDesc
            | PersistedBooksSortMode::MediaPagesCountDesc
            | PersistedBooksSortMode::ReadProgressLastModifiedDateDesc
            | PersistedBooksSortMode::ReadProgressReadDateDesc
            | PersistedBooksSortMode::ReleaseDateDesc
            | PersistedBooksSortMode::NumberSortDesc
            | PersistedBooksSortMode::ReadListNumberDesc
            | PersistedBooksSortMode::RelevanceDesc)).unwrap_or(false);
    for mode in &query.sort_modes {
        let key = match mode {
            PersistedBooksSortMode::TitleAsc => {
                "COALESCE(bm.TITLE, b.NAME) COLLATE icu_names ASC".to_string()
            }
            PersistedBooksSortMode::TitleDesc => {
                "COALESCE(bm.TITLE, b.NAME) COLLATE icu_names DESC".to_string()
            }
            PersistedBooksSortMode::NameAsc => "b.NAME COLLATE icu_names ASC".to_string(),
            PersistedBooksSortMode::NameDesc => "b.NAME COLLATE icu_names DESC".to_string(),
            PersistedBooksSortMode::SeriesTitleAsc => {
                "COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE icu_names ASC".to_string()
            }
            PersistedBooksSortMode::SeriesTitleDesc => {
                "COALESCE(sm.TITLE_SORT, sm.TITLE, s.NAME) COLLATE icu_names DESC".to_string()
            }
            PersistedBooksSortMode::RelevanceAsc | PersistedBooksSortMode::RelevanceDesc => return None,
            PersistedBooksSortMode::CreatedDateAsc => "b.CREATED_DATE ASC".to_string(),
            PersistedBooksSortMode::CreatedDateDesc => "b.CREATED_DATE DESC".to_string(),
            PersistedBooksSortMode::LastModifiedDateAsc => "b.LAST_MODIFIED_DATE ASC".to_string(),
            PersistedBooksSortMode::LastModifiedDateDesc => "b.LAST_MODIFIED_DATE DESC".to_string(),
            PersistedBooksSortMode::FileSizeAsc => "b.FILE_SIZE ASC".to_string(),
            PersistedBooksSortMode::FileSizeDesc => "b.FILE_SIZE DESC".to_string(),
            PersistedBooksSortMode::FileHashAsc => "b.FILE_HASH ASC".to_string(),
            PersistedBooksSortMode::FileHashDesc => "b.FILE_HASH DESC".to_string(),
            PersistedBooksSortMode::UrlAsc => "b.URL ASC".to_string(),
            PersistedBooksSortMode::UrlDesc => "b.URL DESC".to_string(),
            PersistedBooksSortMode::MediaStatusAsc => "COALESCE(m.STATUS, 'UNKNOWN') ASC".to_string(),
            PersistedBooksSortMode::MediaStatusDesc => "COALESCE(m.STATUS, 'UNKNOWN') DESC".to_string(),
            PersistedBooksSortMode::MediaCommentAsc => "COALESCE(m.COMMENT, '') ASC".to_string(),
            PersistedBooksSortMode::MediaCommentDesc => "COALESCE(m.COMMENT, '') DESC".to_string(),
            PersistedBooksSortMode::MediaTypeAsc => "COALESCE(m.MEDIA_TYPE, '') ASC".to_string(),
            PersistedBooksSortMode::MediaTypeDesc => "COALESCE(m.MEDIA_TYPE, '') DESC".to_string(),
            PersistedBooksSortMode::MediaPagesCountAsc => "COALESCE(m.PAGE_COUNT, 0) ASC".to_string(),
            PersistedBooksSortMode::MediaPagesCountDesc => "COALESCE(m.PAGE_COUNT, 0) DESC".to_string(),
            PersistedBooksSortMode::ReadProgressLastModifiedDateAsc
            | PersistedBooksSortMode::ReadProgressLastModifiedDateDesc => {
                if order.rp_user.is_some() {
                    let direction = if matches!(
                        mode,
                        PersistedBooksSortMode::ReadProgressLastModifiedDateAsc
                    ) {
                        "ASC"
                    } else {
                        "DESC"
                    };
                    format!("rp.LAST_MODIFIED_DATE {direction}")
                } else {
                    "NULL".to_string()
                }
            }
            PersistedBooksSortMode::ReadProgressReadDateAsc
            | PersistedBooksSortMode::ReadProgressReadDateDesc => {
                if order.rp_user.is_some() {
                    let direction = if matches!(mode, PersistedBooksSortMode::ReadProgressReadDateAsc)
                    {
                        "ASC"
                    } else {
                        "DESC"
                    };
                    format!("rp.READ_DATE {direction}")
                } else {
                    "NULL".to_string()
                }
            }
            PersistedBooksSortMode::ReleaseDateAsc => "bm.RELEASE_DATE ASC".to_string(),
            PersistedBooksSortMode::ReleaseDateDesc => "bm.RELEASE_DATE DESC".to_string(),
            PersistedBooksSortMode::NumberSortAsc => "COALESCE(bm.NUMBER_SORT, 0) ASC".to_string(),
            PersistedBooksSortMode::NumberSortDesc => "COALESCE(bm.NUMBER_SORT, 0) DESC".to_string(),
            PersistedBooksSortMode::SeriesIdAsc => "b.SERIES_ID ASC".to_string(),
            PersistedBooksSortMode::ReadListNumberAsc | PersistedBooksSortMode::ReadListNumberDesc => {
                if order.readlist_id.is_some() {
                    let direction = if matches!(mode, PersistedBooksSortMode::ReadListNumberAsc) {
                        "ASC"
                    } else {
                        "DESC"
                    };
                    format!("rb.NUMBER {direction}")
                } else {
                    "NULL".to_string()
                }
            }
        };
        if !order.sql.is_empty() {
            order.sql.push_str(", ");
        }
        order.sql.push_str(&key);
    }
    // Engine tail: series_id ASC, then number_sort (direction of last mode), then id ASC.
    order.sql.push_str(", b.SERIES_ID ASC, COALESCE(bm.NUMBER_SORT, 0) ");
    order.sql.push_str(if fallback_desc { "DESC" } else { "ASC" });
    order.sql.push_str(", b.ID ASC");
    Some(order)
}

fn compilable_regex(pattern: &str) -> bool {
    regex::RegexBuilder::new(pattern)
        .case_insensitive(true)
        .build()
        .is_ok()
}

fn series_restrictions(context: &DiscoveryQueryContext) -> Option<SqlExpr> {
    let mut e = SqlExpr::default();
    let mut any = false;
    if let Some(ids) = context.authorized_library_ids.as_ref().filter(|ids| !ids.is_empty()) {
        if any {
            e.push(" AND ");
        }
        push_text_in_list(&mut e, "s.LIBRARY_ID", ids, false);
        any = true;
    }
    if let Some(restrictions) = context.restrictions.as_ref() {
        if let (Some(age), Some(AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
        {
            if any {
                e.push(" AND ");
            }
            e.push("(sm.AGE_RATING IS NULL OR sm.AGE_RATING < ");
            e.bind_int(i64::from(age));
            e.push(")");
            any = true;
        }
        if !restrictions.labels_allow.is_empty() {
            if any {
                e.push(" AND ");
            }
            e.push("EXISTS (SELECT 1 FROM SERIES_METADATA_SHARING sms WHERE sms.SERIES_ID = s.ID AND LOWER(sms.LABEL) IN (");
            for (index, label) in restrictions.labels_allow.iter().enumerate() {
                if index > 0 {
                    e.push(", ");
                }
                e.bind_text(&label.to_ascii_lowercase());
            }
            e.push("))");
            any = true;
        }
        if !restrictions.labels_exclude.is_empty() {
            if any {
                e.push(" AND ");
            }
            e.push("NOT EXISTS (SELECT 1 FROM SERIES_METADATA_SHARING sms WHERE sms.SERIES_ID = s.ID AND LOWER(sms.LABEL) IN (");
            for (index, label) in restrictions.labels_exclude.iter().enumerate() {
                if index > 0 {
                    e.push(", ");
                }
                e.bind_text(&label.to_ascii_lowercase());
            }
            e.push("))");
            any = true;
        }
    }
    if any {
        Some(e)
    } else {
        None
    }
}

fn books_restrictions(
    context: &DiscoveryQueryContext,
    query: &PersistedBooksBrowseQuery,
) -> Option<SqlExpr> {
    let mut e = SqlExpr::default();
    let mut any = false;
    if let Some(ids) = context.authorized_library_ids.as_ref().filter(|ids| !ids.is_empty()) {
        if any {
            e.push(" AND ");
        }
        push_text_in_list(&mut e, "b.LIBRARY_ID", ids, false);
        any = true;
    }
    if let Some(restrictions) = context.restrictions.as_ref() {
        if let (Some(age), Some(AgeRestrictionKind::Exclude)) =
            (restrictions.age, restrictions.age_restriction)
        {
            if any {
                e.push(" AND ");
            }
            e.push("(sm.AGE_RATING IS NULL OR sm.AGE_RATING < ");
            e.bind_int(i64::from(age));
            e.push(")");
            any = true;
        }
    }
    if let Some(library_ids) = query.filters.library_ids.as_ref().filter(|ids| !ids.is_empty()) {
        if any {
            e.push(" AND ");
        }
        push_text_in_list(&mut e, "b.LIBRARY_ID", library_ids, false);
        any = true;
    }
    if any {
        Some(e)
    } else {
        None
    }
}

fn bind_query<'q>(
    mut query: sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments>,
    binds: &[SqlBind],) -> sqlx::query::Query<'q, Sqlite, sqlx::sqlite::SqliteArguments> {
    for bind in binds {
        query = match bind {
            SqlBind::Text(value) => query.bind(value),
            SqlBind::Int(value) => query.bind(value),
            SqlBind::Real(value) => query.bind(value),
        };
    }
    query
}

async fn run_count(pool: &SqlitePool, sql: &str, binds: &[SqlBind]) -> anyhow::Result<usize> {
    let row = bind_query(sqlx::query(sqlx::AssertSqlSafe(sql)), binds)
        .fetch_one(pool)
        .await?;
    Ok(row.get::<i64, _>("COUNT").max(0) as usize)
}

async fn run_ids(pool: &SqlitePool, sql: &str, binds: &[SqlBind]) -> anyhow::Result<Vec<String>> {
    let rows = bind_query(sqlx::query(sqlx::AssertSqlSafe(sql)), binds)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|row| row.get::<String, _>("ID")).collect())
}

pub(super) async fn try_load_series_page(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    query: &PersistedSeriesBrowseQuery,
) -> anyhow::Result<Option<SqlPage>> {
    let user_id = context.user_id.as_deref();
    let cutoffs = match &query.condition {
        Some(condition) => {
            let mut cutoffs = HashMap::new();
            for days in collect_series_release_date_offsets(condition) {
                cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
            }
            cutoffs
        }
        None => HashMap::new(),
    };
    let condition_expr = match &query.condition {
        Some(condition) => match translate_series_condition(condition, user_id, &cutoffs) {
            Some(expr) => Some(expr),
            None => return Ok(None),
        },
        None => None,
    };
    let order = match series_order_by(query, user_id) {
        Some(order) => order,
        None => return Ok(None),
    };

    let pool = backend.db.read_pool();
    let mut from_body =
        "SERIES s LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID LEFT JOIN BOOK_METADATA_AGGREGATION bma ON bma.SERIES_ID = s.ID"
            .to_string();
    let mut join_binds = Vec::<SqlBind>::new();
    if let Some(user_id) = order.rps_user.as_deref() {
        from_body.push_str(" LEFT JOIN READ_PROGRESS_SERIES rps ON rps.SERIES_ID = s.ID AND rps.USER_ID = ?");
        join_binds.push(SqlBind::Text(user_id.to_string()));
    }
    if let Some(collection_id) = order.collection_id.as_deref() {
        from_body.push_str(" LEFT JOIN COLLECTION_SERIES cs ON cs.SERIES_ID = s.ID AND cs.COLLECTION_ID = ?");
        join_binds.push(SqlBind::Text(collection_id.to_string()));
    }

    let mut where_sql = String::new();
    let mut where_binds = Vec::<SqlBind>::new();
    if let Some(restrictions) = series_restrictions(context) {
        where_sql.push_str(&restrictions.sql);
        where_binds.extend(restrictions.binds);
    }
    if let Some(condition) = condition_expr {
        if !where_sql.is_empty() {
            where_sql.push_str(" AND ");
        }
        where_sql.push_str(&condition.sql);
        where_binds.extend(condition.binds);
    }
    let where_clause = if where_sql.is_empty() {
        "1=1".to_string()
    } else {
        where_sql
    };

    let mut binds_all = Vec::new();
    binds_all.extend(join_binds.iter().cloned());
    binds_all.extend(where_binds.iter().cloned());

    let count_sql = format!("SELECT COUNT(*) AS COUNT FROM (SELECT s.ID FROM {from_body} WHERE {where_clause})");
    let total_elements = run_count(pool, &count_sql, &binds_all).await?;

    let mut page_sql = format!("SELECT s.ID FROM {from_body} WHERE {where_clause}");
    if !order.sql.is_empty() {
        page_sql.push_str(" ORDER BY ");
        page_sql.push_str(&order.sql);
    }
    let mut page_binds = Vec::new();
    page_binds.extend(join_binds.iter().cloned());
    page_binds.extend(where_binds.iter().cloned());
    if !query.unpaged {
        page_sql.push_str(" LIMIT ? OFFSET ?");
        page_binds.push(SqlBind::Int(query.size as i64));
        page_binds.push(SqlBind::Int((query.page * query.size) as i64));
    } else {
        page_sql.push_str(" LIMIT -1");
    }
    let ids = run_ids(pool, &page_sql, &page_binds).await?;

    Ok(Some(SqlPage {
        ids,
        total_elements,
    }))
}

pub(super) async fn try_load_books_page(
    backend: &SqliteDiscoveryBrowseService,
    context: &DiscoveryQueryContext,
    query: &PersistedBooksBrowseQuery,
) -> anyhow::Result<Option<SqlPage>> {
    let user_id = context.user_id.as_deref();
    let cutoffs = match &query.condition {
        Some(condition) => {
            let mut cutoffs = HashMap::new();
            for days in collect_book_release_date_offsets(condition) {
                cutoffs.insert(days, backend.persisted_utc_date_minus_days(days).await?);
            }
            cutoffs
        }
        None => HashMap::new(),
    };
    let condition_expr = match &query.condition {
        Some(condition) => match translate_book_condition(condition, user_id, &cutoffs) {
            Some(expr) => Some(expr),
            None => return Ok(None),
        },
        None => None,
    };
    let order = match books_order_by(query, user_id) {
        Some(order) => order,
        None => return Ok(None),
    };

    let pool = backend.db.read_pool();
    let mut from_body =
        "BOOK b JOIN SERIES s ON s.ID = b.SERIES_ID LEFT JOIN BOOK_METADATA bm ON bm.BOOK_ID = b.ID LEFT JOIN MEDIA m ON m.BOOK_ID = b.ID LEFT JOIN SERIES_METADATA sm ON sm.SERIES_ID = s.ID"
            .to_string();
    let mut join_binds = Vec::<SqlBind>::new();
    if let Some(user_id) = order.rp_user.as_deref() {
        from_body.push_str(" LEFT JOIN READ_PROGRESS rp ON rp.BOOK_ID = b.ID AND rp.USER_ID = ?");
        join_binds.push(SqlBind::Text(user_id.to_string()));
    }
    if let Some(readlist_id) = order.readlist_id.as_deref() {
        from_body.push_str(" LEFT JOIN READLIST_BOOK rb ON rb.BOOK_ID = b.ID AND rb.READLIST_ID = ?");
        join_binds.push(SqlBind::Text(readlist_id.to_string()));
    }

    let mut where_sql = String::new();
    let mut where_binds = Vec::<SqlBind>::new();
    if let Some(restrictions) = books_restrictions(context, query) {
        where_sql.push_str(&restrictions.sql);
        where_binds.extend(restrictions.binds);
    }
    if let Some(condition) = condition_expr {
        if !where_sql.is_empty() {
            where_sql.push_str(" AND ");
        }
        where_sql.push_str(&condition.sql);
        where_binds.extend(condition.binds);
    }
    let where_clause = if where_sql.is_empty() {
        "1=1".to_string()
    } else {
        where_sql
    };

    let mut binds_all = Vec::new();
    binds_all.extend(join_binds.iter().cloned());
    binds_all.extend(where_binds.iter().cloned());

    let count_sql = format!("SELECT COUNT(*) AS COUNT FROM (SELECT b.ID FROM {from_body} WHERE {where_clause})");
    let total_elements = run_count(pool, &count_sql, &binds_all).await?;

    let mut page_sql = format!("SELECT b.ID FROM {from_body} WHERE {where_clause}");
    if !order.sql.is_empty() {
        page_sql.push_str(" ORDER BY ");
        page_sql.push_str(&order.sql);
    }
    let mut page_binds = Vec::new();
    page_binds.extend(join_binds.iter().cloned());
    page_binds.extend(where_binds.iter().cloned());
    if !query.unpaged {
        page_sql.push_str(" LIMIT ? OFFSET ?");
        page_binds.push(SqlBind::Int(query.size as i64));
        page_binds.push(SqlBind::Int((query.page * query.size) as i64));
    } else {
        page_sql.push_str(" LIMIT -1");
    }
    let ids = run_ids(pool, &page_sql, &page_binds).await?;

    Ok(Some(SqlPage {
        ids,
        total_elements,
    }))
}

#[cfg(test)]
mod webui_pushdown_probe {
    use super::*;
    use komga_domain::discovery::{CompositeBookCondition, ReadStatus};

    fn q(condition: BookCondition, sort: Vec<PersistedBooksSortMode>) -> PersistedBooksBrowseQuery {
        PersistedBooksBrowseQuery {
            filters: BooksFilterCriteria { library_ids: None },
            condition: Some(condition),
            search: None,
            page: 0,
            size: 20,
            unpaged: false,
            sort_modes: sort,
        }
    }

    fn lib(v: &str) -> BookCondition {
        BookCondition::Value(BookValueCondition::LibraryId(InclusionCondition::Include(vec![v.to_string().into()])))
    }

    fn all(cs: Vec<BookCondition>) -> BookCondition {
        BookCondition::Composite(CompositeBookCondition { operator: FilterOperator::All, conditions: cs })
    }

    fn any(cs: Vec<BookCondition>) -> BookCondition {
        BookCondition::Composite(CompositeBookCondition { operator: FilterOperator::Any, conditions: cs })
    }

    fn exact_str(s: &str) -> StringCondition {
        StringCondition::Exact(InclusionCondition::Include(vec![s.to_string()]))
    }

    fn contains_str(s: &str) -> StringCondition {
        StringCondition::Contains(InclusionCondition::Include(vec![s.to_string()]))
    }

    fn probe(name: &str, condition: BookCondition, sort: Vec<PersistedBooksSortMode>) {
        let query = q(condition, sort);
        let cond = translate_book_condition(query.condition.as_ref().unwrap(), Some("user1"), &HashMap::new());
        let order = books_order_by(&query, Some("user1"));
        assert!(cond.is_some(), "{name}: condition should push down");
        assert!(order.is_some(), "{name}: order should push down");
    }


    #[test]
    fn probe_all() {
        probe("default_lib_sort", all(vec![lib("lib1")]), vec![PersistedBooksSortMode::SeriesTitleAsc, PersistedBooksSortMode::NumberSortAsc]);
        probe("empty_allof", all(vec![]), vec![PersistedBooksSortMode::SeriesTitleAsc, PersistedBooksSortMode::NumberSortAsc]);
        probe("all_libs_anyof", all(vec![any(vec![lib("a"), lib("b")])]), vec![PersistedBooksSortMode::SeriesTitleAsc, PersistedBooksSortMode::NumberSortAsc]);
        probe("empty_anyof", all(vec![any(vec![])]), vec![PersistedBooksSortMode::SeriesTitleAsc, PersistedBooksSortMode::NumberSortAsc]);
        probe("readstatus", all(vec![lib("l"), BookCondition::Value(BookValueCondition::ReadStatus(ReadStatusCondition::Include(vec![ReadStatus::Unread])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("tag_is", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Tag(exact_str("manga")))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("tag_contains", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Tag(contains_str("manga")))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("author_is", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Author(exact_str("toriyama::artist")))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("author_contains", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Author(contains_str("toriyama")))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("mediaprofile", all(vec![lib("l"), BookCondition::Value(BookValueCondition::MediaProfile(InclusionCondition::Include(vec![MediaProfile::Epub])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("oneshot_true", all(vec![lib("l"), BookCondition::Value(BookValueCondition::OneShot(true))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("deleted_false", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Deleted(false))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("mediastatus", all(vec![lib("l"), BookCondition::Value(BookValueCondition::MediaStatus(InclusionCondition::Include(vec![MediaStatus::Ready])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("genre", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Genre(exact_str("action")))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("publisher", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Publisher(InclusionCondition::Include(vec!["shueisha".to_string()])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("language", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Language(InclusionCondition::Include(vec!["en".to_string()])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("seriesid", all(vec![BookCondition::Value(BookValueCondition::SeriesId(InclusionCondition::Include(vec!["s1".to_string().into()])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("poster", all(vec![lib("l"), BookCondition::Value(BookValueCondition::Poster(InclusionCondition::Include(vec![BookPosterCondition { thumbnail_type: None, selected: None }])))]), vec![PersistedBooksSortMode::SeriesTitleAsc]);
        probe("default_no_sort", all(vec![lib("l")]), vec![]);
        probe("readlist_number", all(vec![lib("l")]), vec![PersistedBooksSortMode::ReadListNumberAsc]);
    }
}
