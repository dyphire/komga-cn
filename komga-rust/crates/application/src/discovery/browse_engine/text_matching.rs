#[derive(Clone, Copy)]
pub(super) enum TextMatchMode {
    Exact,
    Contains,
    StartsWith,
    EndsWith,
}

pub(super) fn normalized_text_matches(
    value: &str,
    expected: &[String],
    mode: TextMatchMode,
) -> bool {
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

pub(super) fn any_normalized_text_matches<'a>(
    values: impl IntoIterator<Item = &'a str>,
    expected: &[String],
    mode: TextMatchMode,
) -> bool {
    values
        .into_iter()
        .any(|value| normalized_text_matches(value, expected, mode))
}

pub(super) fn any_ignore_ascii_case<'a>(
    values: impl IntoIterator<Item = &'a str>,
    expected: &[String],
) -> bool {
    values.into_iter().any(|value| {
        expected
            .iter()
            .any(|candidate| value.eq_ignore_ascii_case(candidate))
    })
}

pub(super) fn matches_optional_value<T>(
    value: Option<T>,
    missing_result: bool,
    predicate: impl FnOnce(T) -> bool,
) -> bool {
    match value {
        Some(value) => predicate(value),
        None => missing_result,
    }
}
