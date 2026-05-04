use crate::helpers::{query_value, query_values};
use axum::http::{StatusCode, Uri};
use komga_domain::common_ids::{LibraryId, ReadListId, SeriesId};
use komga_domain::discovery::{
    BookCondition, BookFilter, BookPosterCondition, BookSort, BookValueCondition,
    CompositeBookCondition, DateCondition, DiscoveryError, FilterOperator, InclusionCondition,
    NumberCondition, ReadStatusCondition, StringCondition,
};
use serde_json::Value;

use super::super::persisted::common_helpers::decode_query_component;

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, ()> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(()),
        None => Ok(None),
    }
}

fn decoded_query_values(query: &str, key: &str) -> Option<Vec<String>> {
    let values = query_values(query, key)
        .into_iter()
        .map(|value| decode_query_component(value.trim()))
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();

    (!values.is_empty()).then_some(values)
}

pub(super) fn normalize_release_date_date_time(raw: &str) -> Option<String> {
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

pub(super) fn build_legacy_books_filter(
    library_ids: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    read_statuses: Option<Vec<String>>,
    media_statuses: Option<Vec<String>>,
    released_after: Option<String>,
) -> BookFilter {
    let mut conditions = Vec::new();

    if let Some(library_ids) = library_ids.filter(|v| !v.is_empty()) {
        conditions.push(BookCondition::Value(BookValueCondition::LibraryId(
            InclusionCondition::Include(library_ids.into_iter().map(LibraryId::from).collect()),
        )));
    }
    if let Some(tags) = tags.filter(|v| !v.is_empty()) {
        conditions.push(BookCondition::Value(BookValueCondition::Tag(
            StringCondition::Exact(InclusionCondition::Include(tags)),
        )));
    }
    if let Some(read_statuses) = read_statuses.filter(|v| !v.is_empty()) {
        conditions.push(BookCondition::Value(BookValueCondition::ReadStatus(
            ReadStatusCondition::Include(read_statuses),
        )));
    }
    if let Some(media_statuses) = media_statuses.filter(|v| !v.is_empty()) {
        conditions.push(BookCondition::Value(BookValueCondition::MediaStatus(
            InclusionCondition::Include(media_statuses),
        )));
    }
    if let Some(released_after) = released_after {
        conditions.push(BookCondition::Value(BookValueCondition::ReleaseDate(
            DateCondition::After(released_after),
        )));
    }

    let condition = match conditions.len() {
        0 => None,
        1 => conditions.into_iter().next(),
        _ => Some(BookCondition::Composite(CompositeBookCondition {
            operator: FilterOperator::All,
            conditions,
        })),
    };

    BookFilter {
        condition,
        direct_browse_book_id: None,
    }
}

pub(super) fn legacy_series_books_book_filter(
    series_id: &str,
    uri: &Uri,
) -> Result<BookFilter, StatusCode> {
    let query = uri.query().unwrap_or_default();
    let mut conditions = vec![BookCondition::Value(BookValueCondition::SeriesId(
        InclusionCondition::Include(vec![SeriesId::from(series_id)]),
    ))];

    if let Some(values) = decoded_query_values(query, "tag") {
        conditions.push(BookCondition::Value(BookValueCondition::Tag(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }
    if let Some(values) = decoded_query_values(query, "read_status") {
        conditions.push(BookCondition::Value(BookValueCondition::ReadStatus(
            ReadStatusCondition::Include(values),
        )));
    }
    if let Some(values) = decoded_query_values(query, "media_status") {
        conditions.push(BookCondition::Value(BookValueCondition::MediaStatus(
            InclusionCondition::Include(values),
        )));
    }
    if let Some(values) = decoded_query_values(query, "author") {
        conditions.push(BookCondition::Value(BookValueCondition::Author(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }

    let deleted = optional_query_bool(query, "deleted").map_err(|()| StatusCode::BAD_REQUEST)?;
    if let Some(deleted) = deleted {
        conditions.push(BookCondition::Value(BookValueCondition::Deleted(deleted)));
    }

    let condition = match conditions.len() {
        1 => conditions.into_iter().next(),
        _ => Some(BookCondition::Composite(CompositeBookCondition {
            operator: FilterOperator::All,
            conditions,
        })),
    };

    Ok(BookFilter {
        condition,
        direct_browse_book_id: None,
    })
}

pub(super) fn legacy_series_books_sort_from_query(uri: &Uri) -> Vec<BookSort> {
    let query = uri.query().unwrap_or_default();
    let sort_values: Vec<String> = query_values(query, "sort")
        .into_iter()
        .map(decode_query_component)
        .collect();
    parse_book_sorts_from_json_values(&sort_values, false)
}

fn parse_book_string_value(condition: &Value, key: &str) -> Option<String> {
    condition
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parse_required_lower_string_value(
    condition: &Value,
    condition_type: &str,
) -> Result<String, DiscoveryError> {
    let value = parse_book_string_value(condition, "value")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if value.is_empty() {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a non-empty value",
        )));
    }
    Ok(value)
}

fn parse_string_condition(
    condition: &Value,
    condition_type: &str,
) -> Result<StringCondition, DiscoveryError> {
    let operator = parse_operator(condition);
    match operator.as_str() {
        "isnull" => Ok(StringCondition::IsEmpty),
        "isnotnull" => Ok(StringCondition::IsNotEmpty),
        "contains" => Ok(StringCondition::Contains(InclusionCondition::Include(
            vec![parse_required_lower_string_value(
                condition,
                condition_type,
            )?],
        ))),
        "isnot" => Ok(StringCondition::Exact(InclusionCondition::Exclude(vec![
            parse_required_lower_string_value(condition, condition_type)?,
        ]))),
        "is" => Ok(StringCondition::Exact(InclusionCondition::Include(vec![
            parse_required_lower_string_value(condition, condition_type)?,
        ]))),
        "beginswith" => Ok(StringCondition::StartsWith(InclusionCondition::Include(
            vec![parse_required_lower_string_value(
                condition,
                condition_type,
            )?],
        ))),
        "endswith" => Ok(StringCondition::EndsWith(InclusionCondition::Include(
            vec![parse_required_lower_string_value(
                condition,
                condition_type,
            )?],
        ))),
        _ => Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported operator for {condition_type}: {operator}",
        ))),
    }
}

fn parse_numeric_value(condition: &Value, condition_type: &str) -> Result<String, DiscoveryError> {
    let value = condition.get("value").ok_or_else(|| {
        DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a numeric value",
        ))
    })?;

    if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
        return Ok(number.to_string());
    }

    let Some(raw) = value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a numeric value",
        )));
    };
    raw.parse::<f64>().map_err(|_| {
        DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a numeric value",
        ))
    })?;
    Ok(raw.to_string())
}

fn parse_u16_value(condition: &Value, condition_type: &str) -> Result<u16, DiscoveryError> {
    condition
        .get("value")
        .and_then(|value| {
            value
                .as_u64()
                .and_then(|number| u16::try_from(number).ok())
                .or_else(|| value.as_str().and_then(|raw| raw.parse::<u16>().ok()))
        })
        .ok_or_else(|| {
            DiscoveryError::InvalidSemantics(format!(
                "{condition_type} filter requires a numeric value",
            ))
        })
}

fn parse_release_date_operand(
    condition: &Value,
    condition_type: &str,
) -> Result<String, DiscoveryError> {
    if let Some(date_time) = parse_book_string_value(condition, "dateTime") {
        return normalize_release_date_date_time(&date_time).ok_or_else(|| {
            DiscoveryError::InvalidSemantics(format!(
                "{condition_type} filter requires a valid dateTime value",
            ))
        });
    }

    let value = parse_book_string_value(condition, "value")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    if value.is_empty() {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires a non-empty value",
        )));
    }
    Ok(value)
}

fn parse_duration_days(condition: &Value, condition_type: &str) -> Result<i64, DiscoveryError> {
    let raw = parse_book_string_value(condition, "duration")
        .unwrap_or_default()
        .trim()
        .to_string();
    let Some(days) = raw
        .strip_prefix('P')
        .and_then(|value| value.strip_suffix('D'))
    else {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires an ISO-8601 day duration",
        )));
    };
    days.parse::<i64>().map_err(|_| {
        DiscoveryError::InvalidSemantics(format!(
            "{condition_type} filter requires an ISO-8601 day duration",
        ))
    })
}

fn parse_author_condition_value(condition: &Value) -> Result<String, DiscoveryError> {
    let Some(value) = condition.get("value") else {
        return Err(DiscoveryError::InvalidSemantics(
            "Author filter requires a non-empty value".to_string(),
        ));
    };

    if let Some(raw) = value.as_str() {
        let value = raw.trim().to_ascii_lowercase();
        if value.is_empty() {
            return Err(DiscoveryError::InvalidSemantics(
                "Author filter requires a non-empty value".to_string(),
            ));
        }
        return Ok(value);
    }

    let name = value
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let role = value
        .get("role")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();

    if name.is_empty() && role.is_empty() {
        return Err(DiscoveryError::InvalidSemantics(
            "Author filter requires a non-empty value".to_string(),
        ));
    }

    if role.is_empty() {
        Ok(name)
    } else {
        Ok(format!("{name}::{role}"))
    }
}

fn parse_poster_condition_value(condition: &Value) -> Result<BookPosterCondition, DiscoveryError> {
    let value = condition.get("value").ok_or_else(|| {
        DiscoveryError::InvalidSemantics("Poster filter requires an object value".to_string())
    })?;
    if !value.is_object() {
        return Err(DiscoveryError::InvalidSemantics(
            "Poster filter requires an object value".to_string(),
        ));
    }

    let thumbnail_type = value
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let selected = value.get("selected").and_then(Value::as_bool);

    if thumbnail_type.is_none() && selected.is_none() {
        return Err(DiscoveryError::InvalidSemantics(
            "Poster filter requires type or selected".to_string(),
        ));
    }

    Ok(BookPosterCondition {
        thumbnail_type,
        selected,
    })
}

fn parse_operator(condition: &Value) -> String {
    condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn parse_single_book_value_condition(
    condition: &Value,
) -> Result<BookValueCondition, DiscoveryError> {
    let condition_type = condition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let operator = parse_operator(condition);

    match condition_type {
        "Title" => Ok(BookValueCondition::Title(parse_string_condition(
            condition, "Title",
        )?)),
        "Deleted" => {
            let value = match operator.as_str() {
                "istrue" => true,
                "isfalse" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Deleted: {operator}",
                    )));
                }
            };
            Ok(BookValueCondition::Deleted(value))
        }
        "OneShot" => {
            let value = match operator.as_str() {
                "istrue" => true,
                "isfalse" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for OneShot: {operator}",
                    )));
                }
            };
            Ok(BookValueCondition::OneShot(value))
        }
        "LibraryId" => {
            if operator != "is" {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for LibraryId: {operator}",
                )));
            }
            let value = parse_book_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "LibraryId filter requires a non-empty value".to_string(),
                ));
            }
            Ok(BookValueCondition::LibraryId(InclusionCondition::Include(
                vec![LibraryId::from(value)],
            )))
        }
        "SeriesId" => {
            let include = match operator.as_str() {
                "is" => true,
                "isnot" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for SeriesId: {operator}",
                    )));
                }
            };
            let value = parse_book_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "SeriesId filter requires a non-empty value".to_string(),
                ));
            }
            let series_id = SeriesId::from(value);
            Ok(BookValueCondition::SeriesId(if include {
                InclusionCondition::Include(vec![series_id])
            } else {
                InclusionCondition::Exclude(vec![series_id])
            }))
        }
        "ReadListId" => {
            let include = match operator.as_str() {
                "is" => true,
                "isnot" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for ReadListId: {operator}",
                    )));
                }
            };
            let value = parse_book_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "ReadListId filter requires a non-empty value".to_string(),
                ));
            }
            Ok(BookValueCondition::ReadListId(if include {
                InclusionCondition::Include(vec![ReadListId::from(value)])
            } else {
                InclusionCondition::Exclude(vec![ReadListId::from(value)])
            }))
        }
        "Tag" => Ok(BookValueCondition::Tag(parse_string_condition(
            condition, "Tag",
        )?)),
        "Genre" => Ok(BookValueCondition::Genre(parse_string_condition(
            condition, "Genre",
        )?)),
        "Language" => {
            let value = parse_required_lower_string_value(condition, "Language")?;
            Ok(BookValueCondition::Language(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Language: {operator}",
                    )));
                }
            }))
        }
        "Publisher" => {
            let value = parse_required_lower_string_value(condition, "Publisher")?;
            Ok(BookValueCondition::Publisher(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Publisher: {operator}",
                    )));
                }
            }))
        }
        "AgeRating" => {
            let value = parse_u16_value(condition, "AgeRating")?;
            Ok(BookValueCondition::AgeRating(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for AgeRating: {operator}",
                    )));
                }
            }))
        }
        "ReadStatus" => {
            let value = parse_required_lower_string_value(condition, "ReadStatus")?;
            Ok(BookValueCondition::ReadStatus(match operator.as_str() {
                "isnot" => ReadStatusCondition::Exclude(vec![value]),
                "is" => ReadStatusCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for ReadStatus: {operator}",
                    )));
                }
            }))
        }
        "MediaProfile" => {
            let value = parse_required_lower_string_value(condition, "MediaProfile")?;
            Ok(BookValueCondition::MediaProfile(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for MediaProfile: {operator}",
                    )));
                }
            }))
        }
        "MediaStatus" => {
            let value = parse_required_lower_string_value(condition, "MediaStatus")?;
            Ok(BookValueCondition::MediaStatus(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "beginswith" | "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for MediaStatus: {operator}",
                    )));
                }
            }))
        }
        "Author" => {
            let value = parse_author_condition_value(condition)?;
            Ok(BookValueCondition::Author(match operator.as_str() {
                "contains" => StringCondition::Contains(InclusionCondition::Include(vec![value])),
                "isnot" => StringCondition::Exact(InclusionCondition::Exclude(vec![value])),
                "is" => StringCondition::Exact(InclusionCondition::Include(vec![value])),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Author: {operator}",
                    )));
                }
            }))
        }
        "Poster" => {
            let value = parse_poster_condition_value(condition)?;
            Ok(BookValueCondition::Poster(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Poster: {operator}",
                    )));
                }
            }))
        }
        "NumberSort" => {
            let value = parse_numeric_value(condition, "NumberSort")?;
            Ok(BookValueCondition::NumberSort(match operator.as_str() {
                "isnot" => NumberCondition::Exact(InclusionCondition::Exclude(vec![value])),
                "is" => NumberCondition::Exact(InclusionCondition::Include(vec![value])),
                "greaterthan" => NumberCondition::GreaterThan(value),
                "lessthan" => NumberCondition::LessThan(value),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for NumberSort: {operator}",
                    )));
                }
            }))
        }
        "ReleaseDate" => Ok(BookValueCondition::ReleaseDate(match operator.as_str() {
            "isnull" => DateCondition::IsEmpty,
            "isnotnull" => DateCondition::IsNotEmpty,
            "is" => DateCondition::Exact(InclusionCondition::Include(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "isnot" => DateCondition::Exact(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "after" | "greaterthan" => {
                DateCondition::After(parse_release_date_operand(condition, "ReleaseDate")?)
            }
            "before" | "lessthan" => {
                DateCondition::Before(parse_release_date_operand(condition, "ReleaseDate")?)
            }
            "beginswith" => DateCondition::StartsWith(InclusionCondition::Include(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "endswith" => DateCondition::EndsWith(InclusionCondition::Include(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "doesnotcontain" => DateCondition::Contains(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "doesnotbeginwith" => DateCondition::StartsWith(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "doesnotendwith" => DateCondition::EndsWith(InclusionCondition::Exclude(vec![
                parse_release_date_operand(condition, "ReleaseDate")?,
            ])),
            "isinthelast" => {
                DateCondition::WithinLastDays(parse_duration_days(condition, "ReleaseDate")?)
            }
            "isnotinthelast" => {
                DateCondition::OutsideLastDays(parse_duration_days(condition, "ReleaseDate")?)
            }
            _ => {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for ReleaseDate: {operator}",
                )));
            }
        })),
        "AllOfBook" | "AnyOfBook" => Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} is a composite condition and must not appear in parse_single_book_value_condition",
        ))),
        other => Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported book condition type: {other}",
        ))),
    }
}

fn legacy_book_condition_type(key: &str) -> Option<&'static str> {
    match key {
        "title" => Some("Title"),
        "deleted" => Some("Deleted"),
        "oneShot" | "oneshot" => Some("OneShot"),
        "libraryId" => Some("LibraryId"),
        "seriesId" => Some("SeriesId"),
        "readListId" => Some("ReadListId"),
        "tag" => Some("Tag"),
        "genre" => Some("Genre"),
        "language" => Some("Language"),
        "publisher" => Some("Publisher"),
        "ageRating" => Some("AgeRating"),
        "readStatus" => Some("ReadStatus"),
        "mediaProfile" => Some("MediaProfile"),
        "mediaStatus" => Some("MediaStatus"),
        "author" => Some("Author"),
        "poster" => Some("Poster"),
        "numberSort" => Some("NumberSort"),
        "releaseDate" => Some("ReleaseDate"),
        _ => None,
    }
}

fn parse_legacy_keyed_book_condition(
    condition: &Value,
) -> Option<Result<BookCondition, DiscoveryError>> {
    let object = condition.as_object()?;
    if object.len() != 1 {
        return None;
    }

    let (key, value) = object.iter().next()?;
    let condition_type = legacy_book_condition_type(key)?;
    let mut expanded = value.clone();
    let Value::Object(expanded_object) = &mut expanded else {
        return Some(Err(DiscoveryError::InvalidSemantics(format!(
            "{key} filter must be an object",
        ))));
    };
    expanded_object.insert(
        "type".to_string(),
        Value::String(condition_type.to_string()),
    );
    Some(parse_book_condition_from_json(&expanded))
}

fn parse_book_condition_from_json(condition: &Value) -> Result<BookCondition, DiscoveryError> {
    let condition_type = condition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if condition_type.is_empty()
        && let Some(parsed) = parse_legacy_keyed_book_condition(condition)
    {
        return parsed;
    }

    match condition_type {
        "AllOfBook" => {
            let children = condition
                .get("conditions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiscoveryError::InvalidSemantics(
                        "AllOfBook composite filter missing conditions".to_string(),
                    )
                })?;
            let conditions = children
                .iter()
                .map(parse_book_condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BookCondition::Composite(CompositeBookCondition {
                operator: FilterOperator::All,
                conditions,
            }))
        }
        "AnyOfBook" => {
            let children = condition
                .get("conditions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiscoveryError::InvalidSemantics(
                        "AnyOfBook composite filter missing conditions".to_string(),
                    )
                })?;
            let conditions = children
                .iter()
                .map(parse_book_condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(BookCondition::Composite(CompositeBookCondition {
                operator: FilterOperator::Any,
                conditions,
            }))
        }
        _ => {
            let value = parse_single_book_value_condition(condition)?;
            Ok(BookCondition::Value(value))
        }
    }
}

pub(super) fn parse_book_filter_from_json(
    condition: Option<&Value>,
) -> Result<BookFilter, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(BookFilter {
            condition: None,
            direct_browse_book_id: None,
        });
    };

    let parsed = parse_book_condition_from_json(condition)?;
    Ok(BookFilter {
        condition: Some(parsed),
        direct_browse_book_id: None,
    })
}

pub(super) fn parse_book_sorts_from_json(sorts: Option<&Value>, has_search: bool) -> Vec<BookSort> {
    let Some(sort_values) = sorts.and_then(Value::as_array) else {
        return vec![];
    };

    let strs: Vec<String> = sort_values
        .iter()
        .filter_map(|v| v.as_str())
        .map(str::to_owned)
        .collect();
    parse_book_sorts_from_json_values(&strs, has_search)
}

pub(super) fn parse_book_sorts_from_json_values(
    sorts: &[String],
    has_search: bool,
) -> Vec<BookSort> {
    sorts
        .iter()
        .filter_map(|s| {
            let trimmed = s.trim();
            match trimmed {
                "metadata.title,asc" | "title,asc" | "title" => Some(BookSort::MetadataTitleAsc),
                "metadata.title,desc" | "title,desc" => Some(BookSort::MetadataTitleDesc),
                "createdDate,desc" | "created,desc" => Some(BookSort::CreatedDateDesc),
                "lastModifiedDate,desc" | "lastModified,desc" => {
                    Some(BookSort::LastModifiedDateDesc)
                }
                "readProgress.lastModified,asc" => Some(BookSort::ReadProgressLastModifiedAsc),
                "readProgress.lastModified,desc" | "readProgress.lastModified" => {
                    Some(BookSort::ReadProgressLastModifiedDesc)
                }
                "readProgress.readDate,asc" => Some(BookSort::ReadProgressReadDateAsc),
                "readProgress.readDate,desc" | "readProgress.readDate" => {
                    Some(BookSort::ReadProgressReadDateDesc)
                }
                "metadata.releaseDate,desc" => Some(BookSort::ReleaseDateDesc),
                "metadata.numberSort,asc" | "number,asc" | "series,metadata.numberSort,asc" => {
                    Some(BookSort::NumberSortAsc)
                }
                "seriesId,asc" => Some(BookSort::SeriesIdAsc),
                "relevance,asc" if has_search => Some(BookSort::RelevanceAsc),
                "relevance,desc" if has_search => Some(BookSort::RelevanceDesc),
                _ => None,
            }
        })
        .collect()
}
