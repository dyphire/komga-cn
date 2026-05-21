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

pub fn normalized_text_matches(value: &str, expected: &[String], mode: TextMatchMode) -> bool {
    let normalized = value.to_ascii_lowercase();
    match mode {
        TextMatchMode::Exact => expected.contains(&normalized),
        TextMatchMode::Contains => expected
            .iter()
            .any(|candidate| normalized.contains(candidate.as_str())),
        TextMatchMode::StartsWith => expected
            .iter()
            .any(|candidate| normalized.starts_with(candidate.as_str())),
        TextMatchMode::EndsWith => expected
            .iter()
            .any(|candidate| normalized.ends_with(candidate.as_str())),
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

pub fn matches_any_regex(value: &str, patterns: &[regex::Regex]) -> bool {
    patterns.iter().any(|regex| regex.is_match(value))
}

pub fn compile_case_insensitive_regexes(
    patterns: &[String],
    field: &str,
) -> Result<Vec<regex::Regex>, String> {
    patterns
        .iter()
        .map(|pattern| {
            regex::RegexBuilder::new(pattern)
                .case_insensitive(true)
                .build()
                .map_err(|error| format!("invalid {field} regex `{pattern}`: {error}"))
        })
        .collect()
}
