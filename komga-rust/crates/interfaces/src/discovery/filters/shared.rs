use super::*;

pub(super) fn parse_author_match_value(value: Option<&Value>) -> Option<String> {
    let value = value?;
    if let Some(raw) = value.as_str() {
        let normalized = raw.trim().to_ascii_lowercase();
        return (!normalized.is_empty()).then_some(normalized);
    }

    let object = value.as_object()?;
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty());
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(|v| v.trim().to_ascii_lowercase())
        .filter(|v| !v.is_empty());

    match (name, role) {
        (None, None) => None,
        (Some(name), None) => Some(name),
        (None, Some(role)) => Some(format!("::{role}")),
        (Some(name), Some(role)) => Some(format!("{name}::{role}")),
    }
}

pub(crate) fn normalize_release_date_date_time(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.len() >= 10 {
        &trimmed[..10]
    } else {
        trimmed
    };

    let bytes = candidate.as_bytes();
    if bytes.len() != 10
        || !bytes[0].is_ascii_digit()
        || !bytes[1].is_ascii_digit()
        || !bytes[2].is_ascii_digit()
        || !bytes[3].is_ascii_digit()
        || bytes[4] != b'-'
        || !bytes[5].is_ascii_digit()
        || !bytes[6].is_ascii_digit()
        || bytes[7] != b'-'
        || !bytes[8].is_ascii_digit()
        || !bytes[9].is_ascii_digit()
    {
        return None;
    }

    Some(candidate.to_string())
}

pub(crate) fn parse_iso8601_duration_to_days(raw: &str) -> Option<i64> {
    let mut text = raw.trim();
    if text.is_empty() {
        return None;
    }

    let mut sign = 1.0_f64;
    if let Some(stripped) = text.strip_prefix('-') {
        sign = -1.0;
        text = stripped;
    } else if let Some(stripped) = text.strip_prefix('+') {
        text = stripped;
    }

    let stripped = text.strip_prefix('P')?;

    let mut in_time = false;
    let mut number = String::new();
    let mut total_seconds = 0.0_f64;

    for ch in stripped.chars() {
        if ch == 'T' {
            in_time = true;
            continue;
        }

        if ch.is_ascii_digit() || ch == '.' {
            number.push(ch);
            continue;
        }

        if number.is_empty() {
            return None;
        }

        let parsed = number.parse::<f64>().ok()?;
        number.clear();

        match ch {
            'D' => {
                total_seconds += parsed * 86_400.0;
            }
            'H' if in_time => {
                total_seconds += parsed * 3_600.0;
            }
            'M' if in_time => {
                total_seconds += parsed * 60.0;
            }
            'S' if in_time => {
                total_seconds += parsed;
            }
            _ => return None,
        }
    }

    if !number.is_empty() {
        return None;
    }

    Some(((sign * total_seconds) / 86_400.0).trunc() as i64)
}

pub(super) fn ensure_series_operator(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
) -> Result<(), DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Ok(());
    };
    let op = operator.to_ascii_lowercase();
    let is_supported = if expected_operator == "contains_or_is" {
        op == "contains" || op == "is"
    } else {
        op == expected_operator
    };

    if is_supported || !mode.is_strict() {
        Ok(())
    } else {
        Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported operator for {filter_name}: {operator}",
        )))
    }
}

pub(super) fn ensure_books_operator(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
) -> Result<(), DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        return Ok(());
    };
    let op = operator.to_ascii_lowercase();
    let is_supported = if expected_operator == "contains_or_is" {
        op == "contains" || op == "is"
    } else {
        op == expected_operator
    };

    if is_supported || !mode.is_strict() {
        Ok(())
    } else {
        Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported operator for {filter_name}: {operator}",
        )))
    }
}

pub(super) fn merge_string_groups(groups: Vec<Vec<String>>, all_of: bool) -> Option<Vec<String>> {
    if groups.is_empty() {
        return None;
    }

    if all_of {
        let mut intersection = groups[0].clone();
        for group in groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    }
}

pub(super) fn merge_u16_groups(groups: Vec<Vec<u16>>, all_of: bool) -> Option<Vec<u16>> {
    if groups.is_empty() {
        return None;
    }

    if all_of {
        let mut intersection = groups[0].clone();
        for group in groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    }
}

pub(super) fn merge_u16_lower_bound(bounds: Vec<u16>, all_of: bool) -> Option<u16> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().max()
    } else {
        bounds.into_iter().min()
    }
}

pub(super) fn merge_u16_upper_bound(bounds: Vec<u16>, all_of: bool) -> Option<u16> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().min()
    } else {
        bounds.into_iter().max()
    }
}

pub(super) fn merge_f64_groups(groups: Vec<Vec<f64>>, all_of: bool) -> Option<Vec<f64>> {
    if groups.is_empty() {
        return None;
    }

    if all_of {
        let mut intersection = groups[0].clone();
        for group in groups.iter().skip(1) {
            intersection.retain(|candidate| group.contains(candidate));
        }
        Some(intersection)
    } else {
        let mut union = vec![];
        for group in groups {
            for candidate in group {
                if !union.contains(&candidate) {
                    union.push(candidate);
                }
            }
        }
        Some(union)
    }
}

pub(super) fn merge_release_date_lower_bound(bounds: Vec<String>, all_of: bool) -> Option<String> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().max()
    } else {
        bounds.into_iter().min()
    }
}

pub(super) fn merge_release_date_upper_bound(bounds: Vec<String>, all_of: bool) -> Option<String> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().min()
    } else {
        bounds.into_iter().max()
    }
}

pub(super) fn merge_release_date_in_last_days_bound(bounds: Vec<i64>, all_of: bool) -> Option<i64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().min()
    } else {
        bounds.into_iter().max()
    }
}

pub(super) fn merge_release_date_not_in_last_days_bound(
    bounds: Vec<i64>,
    all_of: bool,
) -> Option<i64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds.into_iter().max()
    } else {
        bounds.into_iter().min()
    }
}

pub(super) fn merge_numeric_lower_bound_f64(bounds: Vec<f64>, all_of: bool) -> Option<f64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds
            .into_iter()
            .max_by(|left, right| left.total_cmp(right))
    } else {
        bounds
            .into_iter()
            .min_by(|left, right| left.total_cmp(right))
    }
}

pub(super) fn merge_numeric_upper_bound_f64(bounds: Vec<f64>, all_of: bool) -> Option<f64> {
    if bounds.is_empty() {
        return None;
    }

    if all_of {
        bounds
            .into_iter()
            .min_by(|left, right| left.total_cmp(right))
    } else {
        bounds
            .into_iter()
            .max_by(|left, right| left.total_cmp(right))
    }
}
