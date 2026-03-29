use super::*;

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

    Ok(RuntimeSeriesFilters {
        library_ids: Some(vec![value.to_string()]),
        ..RuntimeSeriesFilters::default()
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

    Ok(RuntimeSeriesFilters {
        collection_ids: Some(vec![value.to_string()]),
        ..RuntimeSeriesFilters::default()
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
        "is" => RuntimeSeriesFilters {
            titles: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "isnot" => RuntimeSeriesFilters {
            titles_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "contains" => RuntimeSeriesFilters {
            titles_contains: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "doesnotcontain" => RuntimeSeriesFilters {
            titles_contains_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "beginswith" => RuntimeSeriesFilters {
            titles_begins_with: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "doesnotbeginwith" => RuntimeSeriesFilters {
            titles_begins_with_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "endswith" => RuntimeSeriesFilters {
            titles_ends_with: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        _ => RuntimeSeriesFilters {
            titles_ends_with_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
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
        "is" => RuntimeSeriesFilters {
            title_sorts: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "isnot" => RuntimeSeriesFilters {
            title_sorts_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "contains" => RuntimeSeriesFilters {
            title_sorts_contains: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "doesnotcontain" => RuntimeSeriesFilters {
            title_sorts_contains_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "beginswith" => RuntimeSeriesFilters {
            title_sorts_begins_with: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "doesnotbeginwith" => RuntimeSeriesFilters {
            title_sorts_begins_with_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        "endswith" => RuntimeSeriesFilters {
            title_sorts_ends_with: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        _ => RuntimeSeriesFilters {
            title_sorts_ends_with_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
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

    Ok(RuntimeSeriesFilters {
        deleted: Some(deleted),
        ..RuntimeSeriesFilters::default()
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

    Ok(RuntimeSeriesFilters {
        oneshot: Some(oneshot),
        ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            read_statuses_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    Ok(RuntimeSeriesFilters {
        read_statuses: Some(vec![value]),
        ..RuntimeSeriesFilters::default()
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
        |value| RuntimeSeriesFilters {
            genres: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        |value| RuntimeSeriesFilters {
            genres_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        |is_null| RuntimeSeriesFilters {
            genres_null: Some(is_null),
            ..RuntimeSeriesFilters::default()
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
        |value| RuntimeSeriesFilters {
            tags: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        |value| RuntimeSeriesFilters {
            tags_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        |is_null| RuntimeSeriesFilters {
            tags_null: Some(is_null),
            ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            languages_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    Ok(RuntimeSeriesFilters {
        languages: Some(vec![value]),
        ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            publishers_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    Ok(RuntimeSeriesFilters {
        publishers: Some(vec![value]),
        ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            age_ratings_null: Some(true),
            ..RuntimeSeriesFilters::default()
        });
    }
    if operator == "isnotnull" {
        return Ok(RuntimeSeriesFilters {
            age_ratings_null: Some(false),
            ..RuntimeSeriesFilters::default()
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
        "isnot" => Ok(RuntimeSeriesFilters {
            age_ratings_excluded: Some(vec![parsed]),
            ..RuntimeSeriesFilters::default()
        }),
        "greaterthan" => Ok(RuntimeSeriesFilters {
            age_rating_gt: Some(parsed),
            ..RuntimeSeriesFilters::default()
        }),
        "lessthan" => Ok(RuntimeSeriesFilters {
            age_rating_lt: Some(parsed),
            ..RuntimeSeriesFilters::default()
        }),
        _ => Ok(RuntimeSeriesFilters {
            age_ratings: Some(vec![parsed]),
            ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            release_dates_null: Some(true),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(RuntimeSeriesFilters {
            release_dates_null: Some(false),
            ..RuntimeSeriesFilters::default()
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

        return Ok(RuntimeSeriesFilters {
            release_date_gt: Some(date_time),
            ..RuntimeSeriesFilters::default()
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

        return Ok(RuntimeSeriesFilters {
            release_date_lt: Some(date_time),
            ..RuntimeSeriesFilters::default()
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

        return Ok(RuntimeSeriesFilters {
            release_date_in_last_days: Some(days),
            ..RuntimeSeriesFilters::default()
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

        return Ok(RuntimeSeriesFilters {
            release_date_not_in_last_days: Some(days),
            ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            release_date_gt: Some(value),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "lessthan" {
        return Ok(RuntimeSeriesFilters {
            release_date_lt: Some(value),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "beginswith" {
        return Ok(RuntimeSeriesFilters {
            release_date_begins_with: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "endswith" {
        return Ok(RuntimeSeriesFilters {
            release_date_ends_with: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "doesnotcontain" {
        return Ok(RuntimeSeriesFilters {
            release_date_contains_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "doesnotbeginwith" {
        return Ok(RuntimeSeriesFilters {
            release_date_begins_with_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "doesnotendwith" {
        return Ok(RuntimeSeriesFilters {
            release_date_ends_with_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    if operator == "isnot" {
        return Ok(RuntimeSeriesFilters {
            release_dates_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    Ok(RuntimeSeriesFilters {
        release_dates: Some(vec![value]),
        ..RuntimeSeriesFilters::default()
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
        |value| RuntimeSeriesFilters {
            sharing_labels: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        |value| RuntimeSeriesFilters {
            sharing_labels_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        },
        |is_null| RuntimeSeriesFilters {
            sharing_labels_null: Some(is_null),
            ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            series_statuses_excluded: Some(vec![value]),
            ..RuntimeSeriesFilters::default()
        });
    }

    Ok(RuntimeSeriesFilters {
        series_statuses: Some(vec![value]),
        ..RuntimeSeriesFilters::default()
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

    Ok(RuntimeSeriesFilters {
        complete: Some(complete),
        ..RuntimeSeriesFilters::default()
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
            RuntimeSeriesFilters {
                authors: Some(vec![value]),
                ..RuntimeSeriesFilters::default()
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
        return Ok(RuntimeSeriesFilters {
            authors_excluded: Some(vec![encoded]),
            ..RuntimeSeriesFilters::default()
        });
    }

    Ok(RuntimeSeriesFilters {
        authors: Some(vec![encoded]),
        ..RuntimeSeriesFilters::default()
    })
}
