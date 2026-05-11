#[derive(Clone, Copy)]
pub enum TextMatchMode {
    Exact,
    Contains,
    StartsWith,
    EndsWith,
}

pub fn first_group_key(title: &str) -> String {
    title
        .trim()
        .chars()
        .next()
        .map(|ch| ch.to_lowercase().collect::<String>())
        .unwrap_or_else(|| "#".to_string())
}

pub fn filter_rows<T>(rows: Vec<T>, mut predicate: impl FnMut(&T) -> bool) -> Vec<T> {
    rows.into_iter().filter(|row| predicate(row)).collect()
}

/// `expected` is assumed to already be normalized by the request parser.
/// We only normalize the persisted-side value here so callers can share one
/// matching path across exact/contains/prefix/suffix filters without repeating
/// transport parsing rules at every site.
pub fn normalized_text_matches(value: &str, expected: &[String], mode: TextMatchMode) -> bool {
    let normalized = value.to_ascii_lowercase();
    match mode {
        TextMatchMode::Exact => expected.contains(&normalized),
        TextMatchMode::Contains => expected
            .iter()
            .any(|candidate| normalized.contains(candidate)),
        TextMatchMode::StartsWith => expected
            .iter()
            .any(|candidate| normalized.starts_with(candidate)),
        TextMatchMode::EndsWith => expected
            .iter()
            .any(|candidate| normalized.ends_with(candidate)),
    }
}

pub fn any_normalized_text_matches<'a>(
    values: impl IntoIterator<Item = &'a str>,
    expected: &[String],
    mode: TextMatchMode,
) -> bool {
    values
        .into_iter()
        .any(|value| normalized_text_matches(value, expected, mode))
}

pub fn any_ignore_ascii_case<'a>(
    values: impl IntoIterator<Item = &'a str>,
    expected: &[String],
) -> bool {
    values.into_iter().any(|value| {
        expected
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    })
}

pub fn matches_optional_value<T>(
    value: Option<T>,
    missing_result: bool,
    predicate: impl FnOnce(T) -> bool,
) -> bool {
    match value {
        Some(value) => predicate(value),
        None => missing_result,
    }
}
