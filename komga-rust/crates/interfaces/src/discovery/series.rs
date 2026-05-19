use super::persisted::common_helpers::{internal_error_response, requested_query_values};
use super::persisted::models::PersistedSeriesSummary;
use super::persisted::series_queries::series_page_payload;
use crate::helpers::{mark_runtime_owned, to_domain_query_context};
use crate::identity_access::auth::Authenticated;
use crate::state::DiscoveryState;
use axum::Json;
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use komga_application::discovery::SeriesReadModel;
use komga_domain::common_ids::{CollectionId, LibraryId};
use komga_domain::discovery::{
    AgeRatingCondition, CompositeSeriesCondition, DateCondition, FilterOperator,
    InclusionCondition, ReadStatusCondition, SeriesCondition, SeriesFilter, SeriesSort,
    SeriesStatusCondition, SeriesValueCondition, StringCondition,
};
use komga_domain::discovery::{DiscoveryError, PageEnvelope};
use serde_json::{Value, json};

fn normalize_kotlin_unpaged_page_shape<T>(mut page: PageEnvelope<T>) -> PageEnvelope<T> {
    let normalized_size = page.total_elements.max(20);
    page.page = 0;
    page.size = normalized_size;
    page.total_pages = if page.total_elements == 0 {
        0
    } else {
        ((page.total_elements - 1) / normalized_size) + 1
    };
    page
}

pub(in crate::discovery) fn parse_legacy_series_sorts(
    sorts: &[String],
    search: Option<&str>,
    collection_ids: Option<&Vec<String>>,
) -> Vec<SeriesSort> {
    let has_search = search.map(str::trim).filter(|v| !v.is_empty()).is_some();
    let mut result = sorts
        .iter()
        .filter_map(|sort| match sort.as_str() {
            "metadata.titleSort,asc" | "titleSort,asc" => Some(SeriesSort::MetadataTitleSortAsc),
            "metadata.titleSort,desc" | "titleSort,desc" => Some(SeriesSort::MetadataTitleSortDesc),
            "name,asc" => Some(SeriesSort::NameAsc),
            "name,desc" => Some(SeriesSort::NameDesc),
            "readDate,asc" => Some(SeriesSort::ReadDateAsc),
            "readDate,desc" => Some(SeriesSort::ReadDateDesc),
            "collection.number,asc" => Some(SeriesSort::CollectionNumberAsc),
            "collection.number,desc" => Some(SeriesSort::CollectionNumberDesc),
            "random,asc" | "random,desc" => Some(SeriesSort::Random),
            "createdDate,asc" | "created,asc" => Some(SeriesSort::CreatedDateAsc),
            "createdDate,desc" | "created,desc" => Some(SeriesSort::CreatedDateDesc),
            "lastModifiedDate,asc" | "lastModified,asc" => Some(SeriesSort::LastModifiedDateAsc),
            "lastModifiedDate,desc" | "lastModified,desc" => Some(SeriesSort::LastModifiedDateDesc),
            "booksMetadata.releaseDate,asc" => Some(SeriesSort::ReleaseDateAsc),
            "booksMetadata.releaseDate,desc" => Some(SeriesSort::ReleaseDateDesc),
            "booksCount,asc" => Some(SeriesSort::BooksCountAsc),
            "booksCount,desc" => Some(SeriesSort::BooksCountDesc),
            "relevance,asc" if has_search => Some(SeriesSort::RelevanceAsc),
            "relevance,desc" if has_search => Some(SeriesSort::RelevanceDesc),
            _ => None,
        })
        .collect::<Vec<_>>();
    result.dedup();
    if result.is_empty() && sorts.is_empty() && has_search {
        result.push(SeriesSort::RelevanceAsc);
    }
    // Filter out CollectionNumber sorts if no collection_ids are specified
    if collection_ids.map(|ids| ids.is_empty()).unwrap_or(true) {
        result.retain(|sort| {
            !matches!(
                sort,
                SeriesSort::CollectionNumberAsc | SeriesSort::CollectionNumberDesc
            )
        });
    }
    result
}

async fn series_feed(
    app: &DiscoveryState,
    headers: HeaderMap,
    uri: Uri,
    sorts: Vec<SeriesSort>,
    exclude_newly_added: bool,
    kotlin_unpaged_page_shape: bool,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let resolved = match super::query::resolve_series_feed_request(
        &uri,
        sorts,
        exclude_newly_added,
        kotlin_unpaged_page_shape,
    ) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_series(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let page = if resolved.response.kotlin_unpaged_shape {
                normalize_kotlin_unpaged_page_shape(page)
            } else {
                page
            };
            Json(series_read_model_page_payload(
                page,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub async fn series_latest(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_feed(
        &app,
        headers,
        uri,
        vec![SeriesSort::LastModifiedDateDesc],
        false,
        false,
    )
    .await
}

pub async fn series_deprecated_get(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    let app = &app;
    let query = uri.query().unwrap_or_default();
    let requested_library_ids = requested_query_values(query, "library_id");
    let resolved = match super::query::resolve_deprecated_series_request(&uri) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };
    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(
            &app.identity,
            &headers,
            requested_library_ids.as_deref(),
        )
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_series(&context, resolved.request)
        .await
    {
        Ok(page) => {
            let mut response = Json(series_read_model_page_payload(
                page,
                resolved.response.paged,
                resolved.response.sorted,
            ))
            .into_response();
            mark_runtime_owned(&mut response);
            response
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub async fn series_alphabetical_groups(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let app = &app;
    let resolved = match super::query::resolve_series_alphabetical_groups_request(body) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(context) => context,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    match app
        .discovery_browse
        .list_series_alphabetical_groups(&context, resolved.request)
        .await
    {
        Ok(groups) => Json(Value::Array(groups)).into_response(),
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

pub async fn series_list(
    State(app): State<DiscoveryState>,
    _authenticated: Authenticated,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Response {
    let payload = if body.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    } else {
        match serde_json::from_slice::<Value>(&body) {
            Ok(Value::Object(object)) => Value::Object(object),
            Ok(_) | Err(_) => return StatusCode::BAD_REQUEST.into_response(),
        }
    };

    let interfaces_context = match app
        .discovery_auth
        .resolve_query_context_with_persistence(&app.identity, &headers, None)
        .await
    {
        Some(ctx) => ctx,
        None => return StatusCode::UNAUTHORIZED.into_response(),
    };
    let context = to_domain_query_context(interfaces_context);

    let resolved = match super::query::resolve_series_list_request(&uri, payload) {
        Ok(resolved) => resolved,
        Err(error) => return error.into_response(),
    };

    match app
        .discovery_browse
        .list_series(&context, resolved.request)
        .await
    {
        Ok(page) => Json(series_read_model_page_payload(
            page,
            resolved.response.paged,
            resolved.response.sorted,
        ))
        .into_response(),
        Err(DiscoveryError::InvalidSemantics(e)) => {
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response()
        }
        Err(e) => internal_error_response(format!("{e:?}")),
    }
}

fn parse_string_value(condition: &Value, key: &str) -> Option<String> {
    condition
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn parse_required_lower_string_value(
    condition: &Value,
    condition_type: &str,
) -> Result<String, DiscoveryError> {
    let value = parse_string_value(condition, "value")
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

fn parse_series_string_condition(
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

fn parse_u16_value(condition: &Value, condition_type: &str) -> Result<u16, DiscoveryError> {
    condition
        .get("value")
        .and_then(|v| {
            v.as_u64()
                .map(|n| n as u16)
                .or_else(|| v.as_str().and_then(|s| s.trim().parse::<u16>().ok()))
        })
        .ok_or_else(|| {
            DiscoveryError::InvalidSemantics(format!(
                "{condition_type} filter requires a numeric value",
            ))
        })
}

fn normalize_release_date_date_time(raw: &str) -> Option<String> {
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

fn parse_release_date_operand(
    condition: &Value,
    condition_type: &str,
) -> Result<String, DiscoveryError> {
    if let Some(value) = parse_string_value(condition, "value")
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
    {
        return Ok(value);
    }

    if let Some(value) = condition
        .get("dateTime")
        .and_then(Value::as_str)
        .and_then(normalize_release_date_date_time)
    {
        return Ok(value);
    }

    Err(DiscoveryError::InvalidSemantics(format!(
        "{condition_type} filter requires a non-empty value",
    )))
}

fn parse_duration_days(condition: &Value, condition_type: &str) -> Result<i64, DiscoveryError> {
    let raw = condition
        .get("duration")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let Some(days) = raw
        .strip_prefix('P')
        .and_then(|value| value.strip_suffix('D'))
    else {
        return Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} duration must be an ISO-8601 day duration",
        )));
    };
    days.parse::<i64>().map_err(|_| {
        DiscoveryError::InvalidSemantics(format!(
            "{condition_type} duration must be an ISO-8601 day duration",
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

    let Some(object) = value.as_object() else {
        return Err(DiscoveryError::InvalidSemantics(
            "Author filter value must be a string or object".to_string(),
        ));
    };
    let name = object
        .get("name")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());
    let role = object
        .get("role")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty());

    match (name, role) {
        (Some(name), Some(role)) => Ok(format!("{name}::{role}")),
        (Some(name), None) => Ok(name),
        (None, Some(role)) => Ok(format!("::{role}")),
        (None, None) => Err(DiscoveryError::InvalidSemantics(
            "Author filter requires name or role".to_string(),
        )),
    }
}

fn parse_operator(condition: &Value) -> String {
    condition
        .get("operator")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn parse_single_series_value_condition(
    condition: &Value,
) -> Result<SeriesValueCondition, DiscoveryError> {
    let condition_type = condition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let operator = parse_operator(condition);

    match condition_type {
        "Title" => Ok(SeriesValueCondition::Title(parse_series_string_condition(
            condition, "Title",
        )?)),
        "TitleSort" => Ok(SeriesValueCondition::TitleSort(
            parse_series_string_condition(condition, "TitleSort")?,
        )),
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
            Ok(SeriesValueCondition::Deleted(value))
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
            Ok(SeriesValueCondition::OneShot(value))
        }
        "Complete" => {
            let value = match operator.as_str() {
                "istrue" => true,
                "isfalse" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Complete: {operator}",
                    )));
                }
            };
            Ok(SeriesValueCondition::Complete(value))
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
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "LibraryId filter requires a non-empty value".to_string(),
                ));
            }
            let library_id = LibraryId::from(value);
            Ok(SeriesValueCondition::LibraryId(if include {
                InclusionCondition::Include(vec![library_id])
            } else {
                InclusionCondition::Exclude(vec![library_id])
            }))
        }
        "CollectionId" => {
            let include = match operator.as_str() {
                "is" => true,
                "isnot" => false,
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for CollectionId: {operator}",
                    )));
                }
            };
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .trim()
                .to_string();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "CollectionId filter requires a non-empty value".to_string(),
                ));
            }
            let collection_id = CollectionId::from(value);
            Ok(SeriesValueCondition::CollectionId(if include {
                InclusionCondition::Include(vec![collection_id])
            } else {
                InclusionCondition::Exclude(vec![collection_id])
            }))
        }
        "Genre" => Ok(SeriesValueCondition::Genre(parse_series_string_condition(
            condition, "Genre",
        )?)),
        "Tag" => Ok(SeriesValueCondition::Tag(parse_series_string_condition(
            condition, "Tag",
        )?)),
        "Language" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "Language filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::Language(match operator.as_str() {
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
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "Publisher filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::Publisher(match operator.as_str() {
                "isnot" => InclusionCondition::Exclude(vec![value]),
                "is" => InclusionCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for Publisher: {operator}",
                    )));
                }
            }))
        }
        "AgeRating" => Ok(SeriesValueCondition::AgeRating(match operator.as_str() {
            "isnull" => AgeRatingCondition::IsEmpty,
            "isnotnull" => AgeRatingCondition::IsNotEmpty,
            "isnot" => {
                AgeRatingCondition::Exact(InclusionCondition::Exclude(vec![parse_u16_value(
                    condition,
                    "AgeRating",
                )?]))
            }
            "is" => AgeRatingCondition::Exact(InclusionCondition::Include(vec![parse_u16_value(
                condition,
                "AgeRating",
            )?])),
            "greaterthan" => {
                AgeRatingCondition::GreaterThan(parse_u16_value(condition, "AgeRating")?)
            }
            "lessthan" => AgeRatingCondition::LessThan(parse_u16_value(condition, "AgeRating")?),
            _ => {
                return Err(DiscoveryError::InvalidSemantics(format!(
                    "unsupported operator for AgeRating: {operator}",
                )));
            }
        })),
        "ReadStatus" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "ReadStatus filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::ReadStatus(match operator.as_str() {
                "isnot" => ReadStatusCondition::Exclude(vec![value]),
                "is" => ReadStatusCondition::Include(vec![value]),
                _ => {
                    return Err(DiscoveryError::InvalidSemantics(format!(
                        "unsupported operator for ReadStatus: {operator}",
                    )));
                }
            }))
        }
        "SharingLabel" => Ok(SeriesValueCondition::SharingLabel(
            parse_series_string_condition(condition, "SharingLabel")?,
        )),
        "SeriesStatus" => {
            let value = parse_string_value(condition, "value")
                .unwrap_or_default()
                .to_ascii_lowercase();
            if value.is_empty() {
                return Err(DiscoveryError::InvalidSemantics(
                    "SeriesStatus filter requires a non-empty value".to_string(),
                ));
            }
            Ok(SeriesValueCondition::SeriesStatus(
                match operator.as_str() {
                    "isnot" => SeriesStatusCondition::Exclude(vec![value]),
                    "is" => SeriesStatusCondition::Include(vec![value]),
                    _ => {
                        return Err(DiscoveryError::InvalidSemantics(format!(
                            "unsupported operator for SeriesStatus: {operator}",
                        )));
                    }
                },
            ))
        }
        "Author" => {
            let value = parse_author_condition_value(condition)?;
            Ok(SeriesValueCondition::Author(match operator.as_str() {
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
        "ReleaseDate" => Ok(SeriesValueCondition::ReleaseDate(match operator.as_str() {
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
        "AllOfSeries" | "AnyOfSeries" => Err(DiscoveryError::InvalidSemantics(format!(
            "{condition_type} is a composite condition and must not appear in parse_single_series_value_condition",
        ))),
        other => Err(DiscoveryError::InvalidSemantics(format!(
            "unsupported series condition type: {other}",
        ))),
    }
}

fn legacy_series_condition_type(key: &str) -> Option<&'static str> {
    match key {
        "title" => Some("Title"),
        "titleSort" => Some("TitleSort"),
        "deleted" => Some("Deleted"),
        "oneShot" | "oneshot" => Some("OneShot"),
        "complete" => Some("Complete"),
        "libraryId" => Some("LibraryId"),
        "collectionId" => Some("CollectionId"),
        "genre" => Some("Genre"),
        "tag" => Some("Tag"),
        "language" => Some("Language"),
        "publisher" => Some("Publisher"),
        "ageRating" => Some("AgeRating"),
        "readStatus" => Some("ReadStatus"),
        "sharingLabel" => Some("SharingLabel"),
        "seriesStatus" => Some("SeriesStatus"),
        "author" => Some("Author"),
        "releaseDate" => Some("ReleaseDate"),
        _ => None,
    }
}

fn parse_legacy_keyed_series_condition(
    condition: &Value,
) -> Option<Result<SeriesCondition, DiscoveryError>> {
    let object = condition.as_object()?;
    if object.len() != 1 {
        return None;
    }

    let (key, value) = object.iter().next()?;
    if let Some(operator) = match key.as_str() {
        "allOf" => Some(FilterOperator::All),
        "anyOf" => Some(FilterOperator::Any),
        _ => None,
    } {
        let Some(children) = value.as_array() else {
            return Some(Err(DiscoveryError::InvalidSemantics(format!(
                "{key} composite filter must be an array",
            ))));
        };
        let conditions = children
            .iter()
            .map(parse_series_condition_from_json)
            .collect::<Result<Vec<_>, _>>();
        return Some(conditions.map(|conditions| {
            SeriesCondition::Composite(CompositeSeriesCondition {
                operator,
                conditions,
            })
        }));
    }

    let condition_type = legacy_series_condition_type(key)?;
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
    Some(parse_series_condition_from_json(&expanded))
}

fn parse_series_condition_from_json(condition: &Value) -> Result<SeriesCondition, DiscoveryError> {
    let condition_type = condition
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if condition_type.is_empty()
        && let Some(parsed) = parse_legacy_keyed_series_condition(condition)
    {
        return parsed;
    }

    match condition_type {
        "AllOfSeries" => {
            let children = condition
                .get("conditions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiscoveryError::InvalidSemantics(
                        "AllOfSeries composite filter missing conditions".to_string(),
                    )
                })?;
            let conditions = children
                .iter()
                .map(parse_series_condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::All,
                conditions,
            }))
        }
        "AnyOfSeries" => {
            let children = condition
                .get("conditions")
                .and_then(Value::as_array)
                .ok_or_else(|| {
                    DiscoveryError::InvalidSemantics(
                        "AnyOfSeries composite filter missing conditions".to_string(),
                    )
                })?;
            let conditions = children
                .iter()
                .map(parse_series_condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(SeriesCondition::Composite(CompositeSeriesCondition {
                operator: FilterOperator::Any,
                conditions,
            }))
        }
        _ => {
            let value = parse_single_series_value_condition(condition)?;
            Ok(SeriesCondition::Value(value))
        }
    }
}

pub(in crate::discovery) fn parse_series_filter_from_json(
    condition: Option<&Value>,
) -> Result<SeriesFilter, DiscoveryError> {
    let Some(condition) = condition else {
        return Ok(SeriesFilter { condition: None });
    };

    let parsed = parse_series_condition_from_json(condition)?;
    Ok(SeriesFilter {
        condition: Some(parsed),
    })
}

pub(in crate::discovery) fn parse_series_sorts_from_json(
    sorts: Option<&Value>,
    has_search: bool,
) -> Vec<SeriesSort> {
    let Some(sort_values) = sorts.and_then(Value::as_array) else {
        return parse_series_sorts_from_json_values(&[], has_search);
    };

    let values = sort_values
        .iter()
        .filter_map(|v| v.as_str())
        .map(str::to_string)
        .collect::<Vec<_>>();
    parse_series_sorts_from_json_values(&values, has_search)
}

pub(in crate::discovery) fn parse_series_sorts_from_json_values(
    sorts: &[String],
    has_search: bool,
) -> Vec<SeriesSort> {
    let mut result = sorts
        .iter()
        .filter_map(|s| {
            let trimmed = s.trim();
            match trimmed {
                "metadata.titleSort,asc" | "titleSort,asc" => {
                    Some(SeriesSort::MetadataTitleSortAsc)
                }
                "metadata.titleSort,desc" | "titleSort,desc" => {
                    Some(SeriesSort::MetadataTitleSortDesc)
                }
                "name,asc" => Some(SeriesSort::NameAsc),
                "name,desc" => Some(SeriesSort::NameDesc),
                "createdDate,asc" | "created,asc" => Some(SeriesSort::CreatedDateAsc),
                "createdDate,desc" | "created,desc" => Some(SeriesSort::CreatedDateDesc),
                "lastModifiedDate,asc" | "lastModified,asc" => {
                    Some(SeriesSort::LastModifiedDateAsc)
                }
                "lastModifiedDate,desc" | "lastModified,desc" => {
                    Some(SeriesSort::LastModifiedDateDesc)
                }
                "releaseDate,asc" | "booksMetadata.releaseDate,asc" => {
                    Some(SeriesSort::ReleaseDateAsc)
                }
                "releaseDate,desc" | "booksMetadata.releaseDate,desc" => {
                    Some(SeriesSort::ReleaseDateDesc)
                }
                "booksCount,asc" => Some(SeriesSort::BooksCountAsc),
                "booksCount,desc" => Some(SeriesSort::BooksCountDesc),
                "collectionNumber,asc" => Some(SeriesSort::CollectionNumberAsc),
                "collectionNumber,desc" => Some(SeriesSort::CollectionNumberDesc),
                "readDate,asc" => Some(SeriesSort::ReadDateAsc),
                "readDate,desc" => Some(SeriesSort::ReadDateDesc),
                "random" => Some(SeriesSort::Random),
                "relevance,asc" if has_search => Some(SeriesSort::RelevanceAsc),
                "relevance,desc" if has_search => Some(SeriesSort::RelevanceDesc),
                _ => None,
            }
        })
        .collect::<Vec<_>>();
    result.dedup();
    if result.is_empty() && sorts.is_empty() && has_search {
        result.push(SeriesSort::RelevanceAsc);
    }
    result
}

fn series_read_model_to_persisted(model: &SeriesReadModel) -> PersistedSeriesSummary {
    PersistedSeriesSummary {
        id: model.id.clone(),
        library_id: model.library_id.clone(),
        name: model.name.clone(),
        title: model.title.clone(),
        title_sort: model.title_sort.clone(),
        labels: model.labels.clone(),
        created: model.created.clone(),
        last_modified: model.last_modified.clone(),
        file_last_modified: model.file_last_modified.clone(),
        books_count: model.books_count,
        books_read_count: model.books_read_count,
        books_unread_count: model.books_unread_count,
        books_in_progress_count: model.books_in_progress_count,
        status: model.status.clone(),
        summary: model.summary.clone(),
        reading_direction: model.reading_direction.clone(),
        publisher: model.publisher.clone(),
        age_rating: model.age_rating,
        language: model.language.clone(),
        genres: model.genres.clone(),
        tags: model.tags.clone(),
        alternate_titles: model.alternate_titles.clone(),
        metadata_created: model.metadata_created.clone(),
        metadata_last_modified: model.metadata_last_modified.clone(),
        books_metadata_authors: model.books_metadata_authors.clone(),
        books_metadata_tags: model.books_metadata_tags.clone(),
        books_metadata_release_date: model.books_metadata_release_date.clone(),
        books_metadata_summary: model.books_metadata_summary.clone(),
        books_metadata_summary_number: model.books_metadata_summary_number.clone(),
        books_metadata_created: model.books_metadata_created.clone(),
        books_metadata_last_modified: model.books_metadata_last_modified.clone(),
        deleted: model.deleted,
        oneshot: model.oneshot,
    }
}

pub(super) fn series_read_model_page_payload(
    page: PageEnvelope<SeriesReadModel>,
    paged: bool,
    sorted: bool,
) -> Value {
    let converted = PageEnvelope {
        content: page
            .content
            .iter()
            .map(series_read_model_to_persisted)
            .collect(),
        page: page.page,
        size: page.size,
        total_elements: page.total_elements,
        total_pages: page.total_pages,
    };
    series_page_payload(converted, paged, sorted)
}

pub async fn series_new(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_feed(
        &app,
        headers,
        uri,
        vec![SeriesSort::CreatedDateDesc],
        false,
        false,
    )
    .await
}

pub async fn series_updated(
    State(app): State<DiscoveryState>,
    _: Authenticated,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    series_feed(
        &app,
        headers,
        uri,
        vec![SeriesSort::LastModifiedDateDesc],
        true,
        true,
    )
    .await
}
