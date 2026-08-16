use language_tags::LanguageTag;
use std::collections::HashMap;
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn is_valid_calendar_date(year: i32, month: u8, day: u8) -> bool {
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return false,
    };

    (1..=max_day).contains(&day)
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(super) fn normalize_isbn13(value: &str) -> Option<String> {
    let digits = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    if digits.len() != 13 {
        return None;
    }

    let checksum = digits
        .chars()
        .take(12)
        .enumerate()
        .map(|(index, character)| {
            character
                .to_digit(10)
                .expect("isbn digits should stay numeric")
                * if index % 2 == 0 { 1 } else { 3 }
        })
        .sum::<u32>();
    let check_digit = (10 - (checksum % 10)) % 10;
    let actual_check_digit = digits.chars().nth(12)?.to_digit(10)?;
    (check_digit == actual_check_digit).then_some(digits)
}

pub(super) fn generated_readlist_id(name: &str) -> String {
    generated_entity_id("readlist", name)
}

pub(super) fn generated_collection_id(name: &str) -> String {
    generated_entity_id("collection", name)
}

fn generated_entity_id(prefix: &str, name: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let slug = name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    format!("{prefix}-{slug}-{timestamp:x}")
}

pub(super) fn most_frequent_owned<T>(values: impl IntoIterator<Item = T>) -> Option<T>
where
    T: Eq + Hash + Clone,
{
    let mut counts = HashMap::<T, FrequencyRank>::new();

    for (index, value) in values.into_iter().enumerate() {
        counts
            .entry(value)
            .or_insert(FrequencyRank {
                count: 0,
                first_index: index,
            })
            .count += 1;
    }

    counts
        .into_iter()
        .max_by(|(_, left), (_, right)| {
            left.count
                .cmp(&right.count)
                .then_with(|| right.first_index.cmp(&left.first_index))
        })
        .map(|(value, _)| value)
}

struct FrequencyRank {
    count: usize,
    first_index: usize,
}

pub(super) fn dedupe_strings_preserve_order(
    values: impl IntoIterator<Item = String>,
) -> Vec<String> {
    let mut output = Vec::new();

    for value in values {
        if !output.iter().any(|existing| existing == &value) {
            output.push(value);
        }
    }

    output
}

pub(super) fn canonicalize_string_set(values: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut output = dedupe_strings_preserve_order(values);
    output.sort_by(|left, right| {
        left.to_ascii_lowercase()
            .cmp(&right.to_ascii_lowercase())
            .then_with(|| left.cmp(right))
    });
    output
}

pub(super) fn split_comicinfo_list(value: Option<String>) -> Vec<String> {
    value
        .unwrap_or_default()
        .split(',')
        .filter_map(|entry| nonblank_string(entry.to_string()))
        .collect::<Vec<_>>()
}

pub(super) fn compute_series_from_series_and_volume(
    series: Option<String>,
    volume: Option<i64>,
) -> Option<String> {
    let series = series.and_then(nonblank_string)?;
    Some(match volume {
        Some(1) | None => series,
        Some(volume) => format!("{series} ({volume})"),
    })
}

pub(super) fn normalize_optional_bcp47_language(value: Option<String>) -> Option<String> {
    let value = value.and_then(nonblank_string)?;
    let tag = LanguageTag::parse(&value).ok()?;
    let primary_language = tag.primary_language();

    if !(2..=3).contains(&primary_language.len())
        || primary_language
            .bytes()
            .any(|byte| !byte.is_ascii_alphabetic())
        || tag.validate().is_err()
        || ("qaa"..="qtz").contains(&primary_language)
    {
        return None;
    }

    Some(tag.into_string())
}

pub(super) fn normalize_comicinfo_age_rating(value: &str) -> Option<u32> {
    let normalized = value.trim().to_ascii_lowercase().replace(' ', "");

    match normalized.as_str() {
        "adultsonly18+" | "r18+" | "x18+" => Some(18),
        "earlychildhood" => Some(3),
        "everyone" | "g" => Some(0),
        "everyone10+" => Some(10),
        "kidstoadults" => Some(6),
        "m" | "mature17+" => Some(17),
        "ma15+" => Some(15),
        "pg" => Some(8),
        "teen" => Some(13),
        _ => None,
    }
}

pub(super) fn nonblank_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}
