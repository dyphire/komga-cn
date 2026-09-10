use komga_domain::common_ids::{LibraryId, ReadListId, SeriesId};
use komga_domain::discovery::{
    BookCondition, BookFilter, BookPosterCondition, BookSort, BookValueCondition,
    CompositeBookCondition, DateCondition, DiscoveryError, FilterOperator, InclusionCondition,
    NumberCondition, ReadStatusCondition, StringCondition,
};
use komga_domain::media_assets::ThumbnailType;
use serde_json::Value;

use super::filter_values::{
    parse_media_profile_value, parse_media_status_prefix, parse_media_status_value,
    parse_media_status_values, parse_read_status_value, parse_read_status_values, parse_u16_value,
};
use super::{
    DiscoveryRequestError, LegacyKeyedConditionEntry, decode_query_component, query_value,
    query_values,
};

fn optional_query_bool(query: &str, key: &str) -> Result<Option<bool>, DiscoveryRequestError> {
    match query_value(query, key) {
        Some(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Some(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Some(_) => Err(DiscoveryRequestError::BadRequest),
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
) -> Result<BookFilter, DiscoveryRequestError> {
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
        let read_statuses = parse_read_status_values(read_statuses, "ReadStatus")
            .map_err(DiscoveryRequestError::from)?;
        conditions.push(BookCondition::Value(BookValueCondition::ReadStatus(
            ReadStatusCondition::Include(read_statuses),
        )));
    }
    if let Some(media_statuses) = media_statuses.filter(|v| !v.is_empty()) {
        let media_statuses = parse_media_status_values(media_statuses, "MediaStatus")
            .map_err(DiscoveryRequestError::from)?;
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

    Ok(BookFilter {
        condition,
        direct_browse_book_id: None,
    })
}

pub(super) fn legacy_series_books_book_filter(
    series_id: &str,
    query: &str,
) -> Result<BookFilter, DiscoveryRequestError> {
    let mut conditions = vec![BookCondition::Value(BookValueCondition::SeriesId(
        InclusionCondition::Include(vec![SeriesId::from(series_id)]),
    ))];

    if let Some(values) = decoded_query_values(query, "tag") {
        conditions.push(BookCondition::Value(BookValueCondition::Tag(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }
    if let Some(values) = decoded_query_values(query, "read_status") {
        let values =
            parse_read_status_values(values, "ReadStatus").map_err(DiscoveryRequestError::from)?;
        conditions.push(BookCondition::Value(BookValueCondition::ReadStatus(
            ReadStatusCondition::Include(values),
        )));
    }
    if let Some(values) = decoded_query_values(query, "media_status") {
        let values = parse_media_status_values(values, "MediaStatus")
            .map_err(DiscoveryRequestError::from)?;
        conditions.push(BookCondition::Value(BookValueCondition::MediaStatus(
            InclusionCondition::Include(values),
        )));
    }
    if let Some(values) = decoded_query_values(query, "author") {
        conditions.push(BookCondition::Value(BookValueCondition::Author(
            StringCondition::Exact(InclusionCondition::Include(values)),
        )));
    }

    let deleted = optional_query_bool(query, "deleted")?;
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

pub(super) fn legacy_series_books_sort_from_query(query: &str) -> Vec<BookSort> {
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
        "doesnotcontain" => Ok(StringCondition::Contains(InclusionCondition::Exclude(
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
        "doesnotbeginwith" => Ok(StringCondition::StartsWith(InclusionCondition::Exclude(
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
        "doesnotendwith" => Ok(StringCondition::EndsWith(InclusionCondition::Exclude(
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
        .map(|value| {
            ThumbnailType::parse(value).ok_or_else(|| {
                DiscoveryError::InvalidSemantics(
                    "Poster filter requires a valid thumbnail type".to_string(),
                )
            })
        })
        .transpose()?;
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
            let include = match operator.as_str() {
                "is" => true,
                "isnot" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for LibraryId: {operator}",
                    )));
                }
            };
            let value = parse_book_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "LibraryId filter requires a non-empty value".to_string(),
                ));
            }
            let library_id = LibraryId::from(value);
            Ok(BookValueCondition::LibraryId(if include {
                InclusionCondition::Include(vec![library_id])
            } else {
                InclusionCondition::Exclude(vec![library_id])
            }))
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
            let value = parse_book_string_value(condition, "value").unwrap_or_default();
            let value = parse_read_status_value(&value, "ReadStatus")?;
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
            let value = parse_book_string_value(condition, "value").unwrap_or_default();
            let value = parse_media_profile_value(&value, "MediaProfile")?;
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
            let value = parse_book_string_value(condition, "value").unwrap_or_default();
            let condition = match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![parse_media_status_value(
                    &value,
                    "MediaStatus",
                )?]),
                "is" => InclusionCondition::Include(vec![parse_media_status_value(
                    &value,
                    "MediaStatus",
                )?]),
                "beginswith" => {
                    InclusionCondition::Include(parse_media_status_prefix(&value, "MediaStatus")?)
                }
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for MediaStatus: {operator}",
                    )));
                }
            };
            Ok(BookValueCondition::MediaStatus(condition))
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
    let entry = LegacyKeyedConditionEntry::parse(condition)?;
    if let Some(operator) = match entry.key {
        "allOf" => Some(FilterOperator::All),
        "anyOf" => Some(FilterOperator::Any),
        _ => None,
    } {
        let Some(children) = entry.value.as_array() else {
            return Some(Err(DiscoveryError::InvalidSemantics(format!(
                "{} composite filter must be an array",
                entry.key,
            ))));
        };
        let conditions = children
            .iter()
            .map(parse_book_condition_from_json)
            .collect::<Result<Vec<_>, _>>();
        return Some(conditions.map(|conditions| {
            BookCondition::Composite(CompositeBookCondition {
                operator,
                conditions,
            })
        }));
    }

    let condition_type = legacy_book_condition_type(entry.key)?;
    let mut expanded = entry.value.clone();
    let Value::Object(expanded_object) = &mut expanded else {
        return Some(Err(DiscoveryError::InvalidSemantics(format!(
            "{} filter must be an object",
            entry.key,
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
        return parse_book_sorts_from_json_values(&[], has_search);
    };

    let strs: Vec<String> = sort_values
        .iter()
        .filter_map(|v| v.as_str())
        .map(str::to_owned)
        .collect();
    parse_book_sorts_from_json_values(&strs, has_search)
}

fn expand_book_sort_string(s: &str) -> Vec<String> {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() <= 2 {
        return vec![s.to_string()];
    }

    let direction = parts.last().map(|p| p.trim()).unwrap_or_default();
    if !direction.eq_ignore_ascii_case("asc") && !direction.eq_ignore_ascii_case("desc") {
        return vec![s.to_string()];
    }

    parts[..parts.len() - 1]
        .iter()
        .map(|field| format!("{},{}", field.trim(), direction))
        .collect()
}

pub(super) fn parse_book_sorts_from_json_values(
    sorts: &[String],
    has_search: bool,
) -> Vec<BookSort> {
    let expanded: Vec<String> = sorts
        .iter()
        .flat_map(|s| expand_book_sort_string(s.trim()))
        .collect();

    let mut result = expanded
        .iter()
        .filter_map(|s| {
            let trimmed = s.trim();
            match trimmed {
                "metadata.title,asc" | "title,asc" | "title" => Some(BookSort::MetadataTitleAsc),
                "metadata.title,desc" | "title,desc" => Some(BookSort::MetadataTitleDesc),
                "name,asc" | "name" => Some(BookSort::NameAsc),
                "name,desc" => Some(BookSort::NameDesc),
                "series,asc" | "series" => Some(BookSort::SeriesTitleAsc),
                "series,desc" => Some(BookSort::SeriesTitleDesc),
                "createdDate,asc" | "created,asc" => Some(BookSort::CreatedDateAsc),
                "createdDate,desc" | "created,desc" => Some(BookSort::CreatedDateDesc),
                "lastModifiedDate,asc" | "lastModified,asc" => Some(BookSort::LastModifiedDateAsc),
                "lastModifiedDate,desc" | "lastModified,desc" => {
                    Some(BookSort::LastModifiedDateDesc)
                }
                "fileSize,asc" | "size,asc" => Some(BookSort::FileSizeAsc),
                "fileSize,desc" | "size,desc" => Some(BookSort::FileSizeDesc),
                "fileHash,asc" | "fileHash" => Some(BookSort::FileHashAsc),
                "fileHash,desc" => Some(BookSort::FileHashDesc),
                "url,asc" | "url" => Some(BookSort::UrlAsc),
                "url,desc" => Some(BookSort::UrlDesc),
                "media.status,asc" | "media.status" => Some(BookSort::MediaStatusAsc),
                "media.status,desc" => Some(BookSort::MediaStatusDesc),
                "media.comment,asc" | "media.comment" => Some(BookSort::MediaCommentAsc),
                "media.comment,desc" => Some(BookSort::MediaCommentDesc),
                "media.mediaType,asc" | "media.mediaType" => Some(BookSort::MediaTypeAsc),
                "media.mediaType,desc" => Some(BookSort::MediaTypeDesc),
                "media.pagesCount,asc" | "media.pagesCount" => Some(BookSort::MediaPagesCountAsc),
                "media.pagesCount,desc" => Some(BookSort::MediaPagesCountDesc),
                "readProgress.lastModified,asc" => Some(BookSort::ReadProgressLastModifiedAsc),
                "readProgress.lastModified,desc" | "readProgress.lastModified" => {
                    Some(BookSort::ReadProgressLastModifiedDesc)
                }
                "readProgress.readDate,asc" => Some(BookSort::ReadProgressReadDateAsc),
                "readProgress.readDate,desc" | "readProgress.readDate" => {
                    Some(BookSort::ReadProgressReadDateDesc)
                }
                "metadata.releaseDate,asc" => Some(BookSort::ReleaseDateAsc),
                "metadata.releaseDate,desc" => Some(BookSort::ReleaseDateDesc),
                "metadata.numberSort,asc" | "number,asc" => Some(BookSort::NumberSortAsc),
                "metadata.numberSort,desc" | "number,desc" => Some(BookSort::NumberSortDesc),
                "seriesId,asc" => Some(BookSort::SeriesIdAsc),
                "readList.number,asc" | "readList.number" => Some(BookSort::ReadListNumberAsc),
                "readList.number,desc" => Some(BookSort::ReadListNumberDesc),
                "relevance,asc" if has_search => Some(BookSort::RelevanceAsc),
                "relevance,desc" if has_search => Some(BookSort::RelevanceDesc),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    result.dedup();
    if result.is_empty() && expanded.is_empty() && has_search {
        result.push(BookSort::RelevanceAsc);
    }
    result
}
