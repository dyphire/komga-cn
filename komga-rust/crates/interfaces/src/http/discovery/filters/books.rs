use super::*;

pub(super) fn parse_books_library_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    Ok(RuntimeBooksFilters {
        library_ids: Some(vec![value.to_string()]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_series_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for SeriesId: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            series_ids_excluded: Some(vec![value.to_string()]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        series_ids: Some(vec![value.to_string()]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_read_list_id_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for ReadListId: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            read_list_ids_excluded: Some(vec![value.to_string()]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        read_list_ids: Some(vec![value.to_string()]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_title_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(RuntimeBooksFilters::default());
    };
    let value = value.to_ascii_lowercase();

    Ok(match operator.as_str() {
        "is" => RuntimeBooksFilters {
            titles: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        "isnot" => RuntimeBooksFilters {
            titles_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        "contains" => RuntimeBooksFilters {
            titles_contains: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        "doesnotcontain" => RuntimeBooksFilters {
            titles_contains_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        "beginswith" => RuntimeBooksFilters {
            titles_begins_with: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        "doesnotbeginwith" => RuntimeBooksFilters {
            titles_begins_with_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        "endswith" => RuntimeBooksFilters {
            titles_ends_with: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
        _ => RuntimeBooksFilters {
            titles_ends_with_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        },
    })
}

pub(super) fn parse_books_deleted_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(
                "missing operator for Deleted".to_string(),
            ));
        }
        return Ok(RuntimeBooksFilters::default());
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
            return Ok(RuntimeBooksFilters::default());
        }
    };

    Ok(RuntimeBooksFilters {
        deleted: Some(deleted),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_oneshot_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let Some(operator) = condition.get("operator").and_then(Value::as_str) else {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(
                "missing operator for OneShot".to_string(),
            ));
        }
        return Ok(RuntimeBooksFilters::default());
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
            return Ok(RuntimeBooksFilters::default());
        }
    };

    Ok(RuntimeBooksFilters {
        oneshot: Some(oneshot),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_genre_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" && operator != "isnull" && operator != "isnotnull" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Genre: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    if operator == "isnull" {
        return Ok(RuntimeBooksFilters {
            genres_null: Some(true),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(RuntimeBooksFilters {
            genres_null: Some(false),
            ..RuntimeBooksFilters::default()
        });
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            genres_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        genres: Some(vec![value]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_tag_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" && operator != "isnull" && operator != "isnotnull" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Tag: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    if operator == "isnull" {
        return Ok(RuntimeBooksFilters {
            tags_null: Some(true),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(RuntimeBooksFilters {
            tags_null: Some(false),
            ..RuntimeBooksFilters::default()
        });
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            tags_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        tags: Some(vec![value]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_language_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            languages_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        languages: Some(vec![value]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_publisher_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            publishers_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        publishers: Some(vec![value]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_age_rating_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    if operator == "isnull" {
        return Ok(RuntimeBooksFilters {
            age_ratings_null: Some(true),
            ..RuntimeBooksFilters::default()
        });
    }
    if operator == "isnotnull" {
        return Ok(RuntimeBooksFilters {
            age_ratings_null: Some(false),
            ..RuntimeBooksFilters::default()
        });
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_u64)
        .map(|value| value as u16)
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    match operator.as_str() {
        "is" => Ok(RuntimeBooksFilters {
            age_ratings: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        }),
        "isnot" => Ok(RuntimeBooksFilters {
            age_ratings_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        }),
        "greaterthan" => Ok(RuntimeBooksFilters {
            age_rating_gt: Some(value),
            ..RuntimeBooksFilters::default()
        }),
        _ => Ok(RuntimeBooksFilters {
            age_rating_lt: Some(value),
            ..RuntimeBooksFilters::default()
        }),
    }
}

pub(super) fn parse_books_read_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(RuntimeBooksFilters::default());
    };
    let normalized = value.to_ascii_lowercase();

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            read_statuses_excluded: Some(vec![normalized]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        read_statuses: Some(vec![normalized]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_media_profile_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for MediaProfile: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(RuntimeBooksFilters::default());
    };
    let normalized = value.to_ascii_lowercase();
    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            media_profiles_excluded: Some(vec![normalized]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        media_profiles: Some(vec![normalized]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_media_status_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" && operator != "beginswith" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for MediaStatus: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            media_statuses_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        media_statuses: Some(vec![value]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_author_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();

    if operator == "contains" {
        return parse_books_string_filter(condition, "Author", "contains_or_is", mode, |value| {
            RuntimeBooksFilters {
                authors: Some(vec![value]),
                ..RuntimeBooksFilters::default()
            }
        });
    }

    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Author: {operator}",
            )));
        }
        return parse_books_string_filter(condition, "Author", "contains_or_is", mode, |_value| {
            RuntimeBooksFilters::default()
        });
    }

    let Some(_encoded) = parse_author_match_value(condition.get("value")) else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            authors_excluded: Some(vec![_encoded]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        authors: Some(vec![_encoded]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_poster_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is" && operator != "isnot" {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for Poster: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition.get("value").and_then(Value::as_object) else {
        return Ok(RuntimeBooksFilters::default());
    };

    let poster_type = value
        .get("type")
        .and_then(Value::as_str)
        .map(|raw| raw.to_ascii_lowercase());
    let poster_selected = value.get("selected").and_then(Value::as_bool);

    if poster_type.is_none() && poster_selected.is_none() {
        return Ok(RuntimeBooksFilters::default());
    }

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            poster_types_excluded: poster_type.map(|value| vec![value]),
            poster_selected_excluded: poster_selected,
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        poster_types: poster_type.map(|value| vec![value]),
        poster_selected,
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_number_sort_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let operator = condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if operator != "is"
        && operator != "isnot"
        && operator != "greaterthan"
        && operator != "lessthan"
    {
        if mode.is_strict() {
            return Err(DiscoveryError::InvalidSemantics(format!(
                "unsupported operator for NumberSort: {operator}",
            )));
        }
        return Ok(RuntimeBooksFilters::default());
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_f64)
        .or_else(|| {
            condition
                .get("value")
                .and_then(Value::as_i64)
                .map(|v| v as f64)
        })
        .or_else(|| {
            condition
                .get("value")
                .and_then(Value::as_u64)
                .map(|v| v as f64)
        })
        .or_else(|| {
            condition
                .get("value")
                .and_then(Value::as_str)
                .and_then(|raw| raw.trim().parse::<f64>().ok())
        })
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "is" {
        return Ok(RuntimeBooksFilters {
            number_sorts: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            number_sorts_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "greaterthan" {
        return Ok(RuntimeBooksFilters {
            number_sort_gt: Some(value),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        number_sort_lt: Some(value),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_release_date_filter(
    condition: &Value,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
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
        return Ok(RuntimeBooksFilters::default());
    }

    if operator == "isnull" {
        return Ok(RuntimeBooksFilters {
            release_dates_null: Some(true),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isnotnull" {
        return Ok(RuntimeBooksFilters {
            release_dates_null: Some(false),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "after" {
        let Some(_date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(RuntimeBooksFilters::default());
        };

        return Ok(RuntimeBooksFilters {
            release_date_gt: Some(_date_time),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "before" {
        let Some(_date_time) = condition
            .get("dateTime")
            .and_then(Value::as_str)
            .and_then(normalize_release_date_date_time)
        else {
            return Ok(RuntimeBooksFilters::default());
        };

        return Ok(RuntimeBooksFilters {
            release_date_lt: Some(_date_time),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isinthelast" {
        let Some(_days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(RuntimeBooksFilters::default());
        };

        return Ok(RuntimeBooksFilters {
            release_date_in_last_days: Some(_days),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isnotinthelast" {
        let Some(_days) = condition
            .get("duration")
            .and_then(Value::as_str)
            .and_then(parse_iso8601_duration_to_days)
        else {
            return Ok(RuntimeBooksFilters::default());
        };

        return Ok(RuntimeBooksFilters {
            release_date_not_in_last_days: Some(_days),
            ..RuntimeBooksFilters::default()
        });
    }

    let Some(value) = condition
        .get("value")
        .and_then(Value::as_str)
        .map(|raw| raw.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    else {
        return Ok(RuntimeBooksFilters::default());
    };

    if operator == "greaterthan" {
        return Ok(RuntimeBooksFilters {
            release_date_gt: Some(value),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "lessthan" {
        return Ok(RuntimeBooksFilters {
            release_date_lt: Some(value),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "beginswith" {
        return Ok(RuntimeBooksFilters {
            release_date_begins_with: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "endswith" {
        return Ok(RuntimeBooksFilters {
            release_date_ends_with: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "doesnotcontain" {
        return Ok(RuntimeBooksFilters {
            release_date_contains_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "doesnotbeginwith" {
        return Ok(RuntimeBooksFilters {
            release_date_begins_with_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "doesnotendwith" {
        return Ok(RuntimeBooksFilters {
            release_date_ends_with_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    if operator == "isnot" {
        return Ok(RuntimeBooksFilters {
            release_dates_excluded: Some(vec![value]),
            ..RuntimeBooksFilters::default()
        });
    }

    Ok(RuntimeBooksFilters {
        release_dates: Some(vec![value]),
        ..RuntimeBooksFilters::default()
    })
}

pub(super) fn parse_books_string_filter<F>(
    condition: &Value,
    filter_name: &str,
    expected_operator: &str,
    mode: OperatorValidationMode,
    build: F,
) -> Result<RuntimeBooksFilters, DiscoveryError>
where
    F: Fn(String) -> RuntimeBooksFilters,
{
    ensure_books_operator(condition, filter_name, expected_operator, mode)?;
    let Some(value) = condition.get("value").and_then(Value::as_str) else {
        return Ok(RuntimeBooksFilters::default());
    };

    Ok(build(value.to_ascii_lowercase()))
}
