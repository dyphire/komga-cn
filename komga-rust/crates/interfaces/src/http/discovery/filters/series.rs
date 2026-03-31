use super::*;

macro_rules! series_filters {
    ($($tt:tt)*) => {
        RuntimeSeriesFilters::from_criteria(SeriesFilterCriteria { $($tt)* })
    };
}

pub(super) fn parse_series_string_filter<F>(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
    build: F,
) -> Result<RuntimeSeriesFilters, DiscoveryError>
where
    F: Fn(String) -> RuntimeSeriesFilters,
{
    ensure_series_operator(condition, filter_name, expected_operator, mode)?;
    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(RuntimeSeriesFilters::default());
    };

    Ok(build(value.to_ascii_lowercase()))
}

pub(super) fn parse_nullable_series_string_filter<FI, FE, FN>(
    condition: &Value,
    mode: OperatorValidationMode,
    filter_name: &str,
    build_include: FI,
    build_exclude: FE,
    build_null: FN,
) -> Result<RuntimeSeriesFilters, DiscoveryError>
where
    FI: Fn(String) -> RuntimeSeriesFilters,
    FE: Fn(String) -> RuntimeSeriesFilters,
    FN: Fn(bool) -> RuntimeSeriesFilters,
{
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if operator != "is"
        && operator != "contains"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for {filter_name}: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    if operator == "isnull" {
        return Ok(build_null(true));
    }
    if operator == "isnotnull" {
        return Ok(build_null(false));
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(RuntimeSeriesFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(build_exclude(normalized));
    }

    Ok(build_include(normalized))
}

pub(super) fn parse_library_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for LibraryId: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    Ok(series_filters! {
        library_ids: Some(vec![value.to_string()]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_collection_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for CollectionId: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    Ok(series_filters! {
        collection_ids: Some(vec![value.to_string()]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_title_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "contains"
        && operator != "doesnotcontain"
        && operator != "beginswith"
        && operator != "doesnotbeginwith"
        && operator != "endswith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Title: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    Ok(match operator.as_str() {
        "is" => series_filters! {
            titles: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "isnot" => series_filters! {
            titles_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "contains" => series_filters! {
            titles_contains: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "doesnotcontain" => series_filters! {
            titles_contains_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "beginswith" => series_filters! {
            titles_begins_with: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "doesnotbeginwith" => series_filters! {
            titles_begins_with_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "endswith" => series_filters! {
            titles_ends_with: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        _ => series_filters! {
            titles_ends_with_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
    })
}

pub(super) fn parse_series_title_sort_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "contains"
        && operator != "doesnotcontain"
        && operator != "beginswith"
        && operator != "doesnotbeginwith"
        && operator != "endswith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for TitleSort: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    Ok(match operator.as_str() {
        "is" => series_filters! {
            title_sorts: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "isnot" => series_filters! {
            title_sorts_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "contains" => series_filters! {
            title_sorts_contains: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "doesnotcontain" => series_filters! {
            title_sorts_contains_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "beginswith" => series_filters! {
            title_sorts_begins_with: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "doesnotbeginwith" => series_filters! {
            title_sorts_begins_with_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        "endswith" => series_filters! {
            title_sorts_ends_with: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
        _ => series_filters! {
            title_sorts_ends_with_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        },
    })
}

pub(super) fn parse_deleted_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(
                "missing operator for Deleted".to_string(),
            ));
        }
        return Ok(RuntimeSeriesFilters::default());
    };

    let deleted = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for Deleted: {operator}",
                )));
            }
            return Ok(RuntimeSeriesFilters::default());
        }
    };

    Ok(series_filters! {
        deleted: Some(deleted),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_oneshot_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(
                "missing operator for OneShot".to_string(),
            ));
        }
        return Ok(RuntimeSeriesFilters::default());
    };

    let oneshot = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for OneShot: {operator}",
                )));
            }
            return Ok(RuntimeSeriesFilters::default());
        }
    };

    Ok(series_filters! {
        oneshot: Some(oneshot),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_read_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for ReadStatus: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(series_filters! {
            read_statuses_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    Ok(series_filters! {
        read_statuses: Some(vec![value]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_genre_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    parse_nullable_series_string_filter(
        condition,
        mode,
        "Genre",
        |value| {
            series_filters! {
                genres: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        },
        |value| {
            series_filters! {
                genres_excluded: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        },
        |is_null| {
            series_filters! {
                genres_null: Some(is_null),
                ..SeriesFilterCriteria::default()
            }
        },
    )
}

pub(super) fn parse_series_tag_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    parse_nullable_series_string_filter(
        condition,
        mode,
        "Tag",
        |value| {
            series_filters! {
                tags: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        },
        |value| {
            series_filters! {
                tags_excluded: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        },
        |is_null| {
            series_filters! {
                tags_null: Some(is_null),
                ..SeriesFilterCriteria::default()
            }
        },
    )
}

pub(super) fn parse_series_language_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Language: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(series_filters! {
            languages_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    Ok(series_filters! {
        languages: Some(vec![value]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_publisher_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Publisher: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(series_filters! {
            publishers_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    Ok(series_filters! {
        publishers: Some(vec![value]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_age_rating_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
        && operator != "greaterthan"
        && operator != "lessthan"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for AgeRating: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    if operator == "isnull" {
        return Ok(series_filters! {
            age_ratings_null: Some(true),
            ..SeriesFilterCriteria::default()
        });
    }
    if operator == "isnotnull" {
        return Ok(series_filters! {
            age_ratings_null: Some(false),
            ..SeriesFilterCriteria::default()
        });
    }

    let Some(value) = condition.get("value") else {
        return Ok(RuntimeSeriesFilters::default());
    };

    let parsed = if let Some(number) = value.as_u64() {
        number as u16
    } else if let Some(raw) = value.as_str() {
        match raw.parse::<u16>() {
            Ok(value) => value,
            Err(_) => return Ok(RuntimeSeriesFilters::default()),
        }
    } else {
        return Ok(RuntimeSeriesFilters::default());
    };

    match operator.as_str() {
        "isnot" => Ok(series_filters! {
            age_ratings_excluded: Some(vec![parsed]),
            ..SeriesFilterCriteria::default()
        }),
        "greaterthan" => Ok(series_filters! {
            age_rating_gt: Some(parsed),
            ..SeriesFilterCriteria::default()
        }),
        "lessthan" => Ok(series_filters! {
            age_rating_lt: Some(parsed),
            ..SeriesFilterCriteria::default()
        }),
        _ => Ok(series_filters! {
            age_ratings: Some(vec![parsed]),
            ..SeriesFilterCriteria::default()
        }),
    }
}

pub(super) fn parse_series_release_date_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "isnull"
        && operator != "isnotnull"
        && operator != "greaterthan"
        && operator != "lessthan"
        && operator != "after"
        && operator != "before"
        && operator != "isinthelast"
        && operator != "isnotinthelast"
        && operator != "beginswith"
        && operator != "endswith"
        && operator != "doesnotcontain"
        && operator != "doesnotbeginwith"
        && operator != "doesnotendwith"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for ReleaseDate: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    if operator == "isnull" {
        return Ok(series_filters! {
            release_dates_null: Some(true),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(series_filters! {
            release_dates_null: Some(false),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "after" {
        let Some(date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(RuntimeSeriesFilters::default());
        };

        return Ok(series_filters! {
            release_date_gt: Some(date_time),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "before" {
        let Some(date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(RuntimeSeriesFilters::default());
        };

        return Ok(series_filters! {
            release_date_lt: Some(date_time),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "isinthelast" {
        let Some(days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(RuntimeSeriesFilters::default());
        };

        return Ok(series_filters! {
            release_date_in_last_days: Some(days),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "isnotinthelast" {
        let Some(days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(RuntimeSeriesFilters::default());
        };

        return Ok(series_filters! {
            release_date_not_in_last_days: Some(days),
            ..SeriesFilterCriteria::default()
        });
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if operator == "greaterthan" {
        return Ok(series_filters! {
            release_date_gt: Some(value),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "lessthan" {
        return Ok(series_filters! {
            release_date_lt: Some(value),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "beginswith" {
        return Ok(series_filters! {
            release_date_begins_with: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "endswith" {
        return Ok(series_filters! {
            release_date_ends_with: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "doesnotcontain" {
        return Ok(series_filters! {
            release_date_contains_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "doesnotbeginwith" {
        return Ok(series_filters! {
            release_date_begins_with_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "doesnotendwith" {
        return Ok(series_filters! {
            release_date_ends_with_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    if operator == "isnot" {
        return Ok(series_filters! {
            release_dates_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    Ok(series_filters! {
        release_dates: Some(vec![value]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_sharing_label_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    parse_nullable_series_string_filter(
        condition,
        mode,
        "SharingLabel",
        |value| {
            series_filters! {
                sharing_labels: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        },
        |value| {
            series_filters! {
                sharing_labels_excluded: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        },
        |is_null| {
            series_filters! {
                sharing_labels_null: Some(is_null),
                ..SeriesFilterCriteria::default()
            }
        },
    )
}

pub(super) fn parse_series_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for SeriesStatus: {operator}",
            )));
        }
        return Ok(RuntimeSeriesFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(series_filters! {
            series_statuses_excluded: Some(vec![value]),
            ..SeriesFilterCriteria::default()
        });
    }

    Ok(series_filters! {
        series_statuses: Some(vec![value]),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_complete_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(
                "missing operator for Complete".to_string(),
            ));
        }
        return Ok(RuntimeSeriesFilters::default());
    };

    let complete = match operator.to_ascii_lowercase().as_str() {
        "istrue" => true,
        "isfalse" => false,
        _ => {
            if mode.is_strict() {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for Complete: {operator}",
                )));
            }
            return Ok(RuntimeSeriesFilters::default());
        }
    };

    Ok(series_filters! {
        complete: Some(complete),
        ..SeriesFilterCriteria::default()
    })
}

pub(super) fn parse_series_author_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if operator == "contains" {
        return parse_series_string_filter(condition, "Author", "contains_or_is", mode, |value| {
            series_filters! {
                authors: Some(vec![value]),
                ..SeriesFilterCriteria::default()
            }
        });
    }

    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Author: {operator}",
            )));
        }
        return parse_series_string_filter(condition, "Author", "contains_or_is", mode, |_value| {
            RuntimeSeriesFilters::default()
        });
    }

    let Some(encoded) = parse_author_match_value(condition.get("value")) else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if operator == "isnot" {
        return Ok(series_filters! {
            authors_excluded: Some(vec![encoded]),
            ..SeriesFilterCriteria::default()
        });
    }

    Ok(series_filters! {
        authors: Some(vec![encoded]),
        ..SeriesFilterCriteria::default()
    })
}
