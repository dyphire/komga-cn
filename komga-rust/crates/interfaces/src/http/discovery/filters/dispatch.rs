use super::*;

pub(super) fn parse_runtime_series_filters_impl(
    condition: Option<&Value>,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    parse_runtime_series_filters_with_mode_impl(condition, OperatorValidationMode::Lenient)
}

pub(super) fn parse_runtime_series_filters_with_mode_impl(
    condition: Option<&Value>,
    mode: OperatorValidationMode,
) -> Result<RuntimeSeriesFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(RuntimeSeriesFilters::default());
    };

    if condition.get("type").and_then(Value::as_str).is_none() {
        let normalized = normalize_series_condition_shape(condition)?;
        return parse_runtime_series_filters_with_mode_impl(Some(&normalized), mode);
    }

    let Some(condition_type) = condition.get("type").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidSemantics(
            "series condition missing type".to_string(),
        ));
    };

    match condition_type {
        "LibraryId" => parse_library_id_filter(condition, mode),
        "CollectionId" => parse_collection_id_filter(condition, mode),
        "Title" => parse_series_title_filter(condition, mode),
        "TitleSort" => parse_series_title_sort_filter(condition, mode),
        "Deleted" => parse_deleted_filter(condition, mode),
        "OneShot" => parse_oneshot_filter(condition, mode),
        "ReadStatus" => parse_series_read_status_filter(condition, mode),
        "Genre" => parse_series_genre_filter(condition, mode),
        "Tag" => parse_series_tag_filter(condition, mode),
        "Language" => parse_series_language_filter(condition, mode),
        "Publisher" => parse_series_publisher_filter(condition, mode),
        "AgeRating" => parse_series_age_rating_filter(condition, mode),
        "ReleaseDate" => parse_series_release_date_filter(condition, mode),
        "SharingLabel" => parse_series_sharing_label_filter(condition, mode),
        "SeriesStatus" => parse_series_status_filter(condition, mode),
        "Complete" => parse_series_complete_filter(condition, mode),
        "Author" => parse_series_author_filter(condition, mode),
        "AllOfSeries" => parse_composite_filters(condition, true, mode),
        "AnyOfSeries" => parse_composite_filters(condition, false, mode),
        unsupported => {
            if mode.is_strict() {
                Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported series condition type: {unsupported}",
                )))
            } else {
                Ok(RuntimeSeriesFilters::default())
            }
        }
    }
}

pub(super) fn parse_runtime_books_filters_impl(
    condition: Option<&Value>,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    parse_runtime_books_filters_with_mode_impl(condition, OperatorValidationMode::Lenient)
}

pub(super) fn parse_runtime_books_filters_with_mode_impl(
    condition: Option<&Value>,
    mode: OperatorValidationMode,
) -> Result<RuntimeBooksFilters, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(RuntimeBooksFilters::default());
    };

    if condition.get("type").and_then(Value::as_str).is_none() {
        let normalized = normalize_books_condition_shape(condition)?;
        return parse_runtime_books_filters_with_mode_impl(Some(&normalized), mode);
    }

    let Some(condition_type) = condition.get("type").and_then(Value::as_str) else {
        return Err(DiscoveryError::InvalidSemantics(
            "books condition missing type".to_string(),
        ));
    };

    match condition_type {
        "LibraryId" => parse_books_library_id_filter(condition, mode),
        "SeriesId" => parse_books_series_id_filter(condition, mode),
        "ReadListId" => parse_books_read_list_id_filter(condition, mode),
        "Title" => parse_books_title_filter(condition, mode),
        "Deleted" => parse_books_deleted_filter(condition, mode),
        "OneShot" => parse_books_oneshot_filter(condition, mode),
        "Genre" => parse_books_genre_filter(condition, mode),
        "Tag" => parse_books_tag_filter(condition, mode),
        "ReadStatus" => parse_books_read_status_filter(condition, mode),
        "MediaProfile" => parse_books_media_profile_filter(condition, mode),
        "MediaStatus" => parse_books_media_status_filter(condition, mode),
        "Language" => parse_books_language_filter(condition, mode),
        "Publisher" => parse_books_publisher_filter(condition, mode),
        "AgeRating" => parse_books_age_rating_filter(condition, mode),
        "Author" => parse_books_author_filter(condition, mode),
        "Poster" => parse_books_poster_filter(condition, mode),
        "NumberSort" => parse_books_number_sort_filter(condition, mode),
        "ReleaseDate" => parse_books_release_date_filter(condition, mode),
        "AllOfBook" => parse_books_composite_filters(condition, true, mode),
        "AnyOfBook" => parse_books_composite_filters(condition, false, mode),
        unsupported => {
            if mode.is_strict() {
                Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported books condition type: {unsupported}",
                )))
            } else {
                Ok(RuntimeBooksFilters::default())
            }
        }
    }
}

pub(super) fn exact_oneshot_bootstrap_series_id_impl(payload: Option<&Value>) -> Option<String> {
    let payload = payload?.as_object()?;
    if payload.len() != 1 {
        return None;
    }

    let condition = payload.get("condition")?.as_object()?;
    if condition.len() != 3 {
        return None;
    }

    if condition.get("type").and_then(Value::as_str) != Some("SeriesId") {
        return None;
    }

    if !condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .eq_ignore_ascii_case("is")
    {
        return None;
    }

    condition
        .get("value")
        .and_then(Value::as_str)
        .map(str::to_string)
}
