use super::*;

pub(super) fn normalize_series_condition_shape(condition: &Value) -> Result<Value, DiscoveryError> {
    let Some(object) = condition.as_object() else {
        return Err(DiscoveryError::InvalidSemantics(
            "series condition must be an object".to_string(),
        ));
    };

    if let Some(children) = object.get("allOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_series_condition_shape)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AllOfSeries", "conditions": conditions }));
    }

    if let Some(children) = object.get("anyOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_series_condition_shape)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AnyOfSeries", "conditions": conditions }));
    }

    map_webui_leaf_to_condition_shape(
        object,
        &[
            ("libraryId", "LibraryId"),
            ("collectionId", "CollectionId"),
            ("title", "Title"),
            ("titleSort", "TitleSort"),
            ("deleted", "Deleted"),
            ("oneShot", "OneShot"),
            ("readStatus", "ReadStatus"),
            ("genre", "Genre"),
            ("tag", "Tag"),
            ("language", "Language"),
            ("publisher", "Publisher"),
            ("ageRating", "AgeRating"),
            ("releaseDate", "ReleaseDate"),
            ("sharingLabel", "SharingLabel"),
            ("seriesStatus", "SeriesStatus"),
            ("complete", "Complete"),
            ("author", "Author"),
        ],
        "series",
    )
}

pub(super) fn normalize_books_condition_shape(condition: &Value) -> Result<Value, DiscoveryError> {
    let Some(object) = condition.as_object() else {
        return Err(DiscoveryError::InvalidSemantics(
            "books condition must be an object".to_string(),
        ));
    };

    if let Some(children) = object.get("allOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_books_condition_shape)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AllOfBook", "conditions": conditions }));
    }

    if let Some(children) = object.get("anyOf").and_then(Value::as_array) {
        let conditions = children
            .iter()
            .map(normalize_books_condition_shape)
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(json!({ "type": "AnyOfBook", "conditions": conditions }));
    }

    map_webui_leaf_to_condition_shape(
        object,
        &[
            ("libraryId", "LibraryId"),
            ("seriesId", "SeriesId"),
            ("readListId", "ReadListId"),
            ("title", "Title"),
            ("deleted", "Deleted"),
            ("oneShot", "OneShot"),
            ("genre", "Genre"),
            ("tag", "Tag"),
            ("readStatus", "ReadStatus"),
            ("mediaProfile", "MediaProfile"),
            ("mediaStatus", "MediaStatus"),
            ("language", "Language"),
            ("publisher", "Publisher"),
            ("ageRating", "AgeRating"),
            ("author", "Author"),
            ("numberSort", "NumberSort"),
            ("releaseDate", "ReleaseDate"),
        ],
        "books",
    )
}

pub(super) fn map_webui_leaf_to_condition_shape(
    object: &serde_json::Map<String, Value>,
    mappings: &[(&str, &str)],
    label: &str,
) -> Result<Value, DiscoveryError> {
    for (webui_key, condition_type) in mappings {
        if let Some(operator_shape) = object.get(*webui_key)
            && let Some(operator_map) = operator_shape.as_object()
        {
            let mut normalized = serde_json::Map::new();
            normalized.insert(
                "type".to_string(),
                Value::String((*condition_type).to_string()),
            );
            for (key, value) in operator_map {
                normalized.insert(key.clone(), value.clone());
            }
            return Ok(Value::Object(normalized));
        }
    }

    let default_condition_type = if label == "books" {
        "AllOfBook"
    } else {
        "AllOfSeries"
    };

    Ok(json!({
        "type": default_condition_type,
        "conditions": [],
    }))
}

pub(super) fn webui_bridge_series_filters_from_payload(
    payload: Option<&Value>,
) -> RuntimeSeriesFilters {
    let mut filters = RuntimeSeriesFilters::default();
    let Some(condition) = payload.and_then(|value| value.get("condition")) else {
        return filters;
    };

    let mut library_ids = vec![];
    collect_webui_string_condition_values(condition, "libraryId", &mut library_ids);
    if !library_ids.is_empty() {
        library_ids.sort();
        library_ids.dedup();
        filters.criteria.library_ids = Some(library_ids);
    }

    filters
}

pub(super) fn webui_bridge_books_filters_from_payload(
    payload: Option<&Value>,
) -> RuntimeBooksFilters {
    let mut filters = RuntimeBooksFilters::default();
    let Some(condition) = payload.and_then(|value| value.get("condition")) else {
        return filters;
    };

    let mut library_ids = vec![];
    collect_webui_string_condition_values(condition, "libraryId", &mut library_ids);
    if !library_ids.is_empty() {
        library_ids.sort();
        library_ids.dedup();
        filters.criteria.library_ids = Some(library_ids);
    }

    let mut series_ids = vec![];
    collect_webui_string_condition_values(condition, "seriesId", &mut series_ids);
    if !series_ids.is_empty() {
        series_ids.sort();
        series_ids.dedup();
        filters.criteria.series_ids = Some(series_ids);
    }

    filters
}

pub(super) fn restrict_series_filters_to_persisted_shape(filters: &mut RuntimeSeriesFilters) {
    filters.criteria.restrict_for_persisted_webui_shape();
}

pub(super) fn restrict_books_filters_to_persisted_shape(filters: &mut RuntimeBooksFilters) {
    filters.direct_browse_family = None;
    filters.criteria.restrict_for_persisted_webui_shape();
}

pub(super) fn collect_webui_string_condition_values(
    condition: &Value,
    key: &str,
    output: &mut Vec<String>,
) {
    match condition {
        Value::Object(object) => {
            if let Some(filter) = object.get(key)
                && let Some(filter_object) = filter.as_object()
                && let Some(value) = filter_object.get("value").and_then(Value::as_str)
                && !value.is_empty()
            {
                output.push(value.to_string());
            }

            for nested in object.values() {
                collect_webui_string_condition_values(nested, key, output);
            }
        }
        Value::Array(values) => {
            for nested in values {
                collect_webui_string_condition_values(nested, key, output);
            }
        }
        _ => {}
    }
}
