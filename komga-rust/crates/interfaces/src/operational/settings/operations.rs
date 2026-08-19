use anyhow::Result;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};

use crate::contracts::common::MessageDto;
use crate::contracts::common::{KotlinLocalDateTime, PageDto};
use crate::contracts::history::HistoryEventDto;
use crate::contracts::operational::OAuth2ClientDto;
use crate::identity_access::auth::{Admin, Authenticated};
use crate::state::OperationalApiState;
use komga_application::identity_access::user_id;
use komga_application::operational::{
    HistoryEvent, HistoryPage, HistorySort, HistorySortDirection, HistorySortProperty,
    HistorySortSelection,
};

use super::{query_value, query_values};

#[derive(Debug, Eq, PartialEq)]
enum SyncpointDeleteScope {
    All,
    ApiKeys(Vec<String>),
}

pub(crate) async fn get_history(
    State(app): State<OperationalApiState>,
    _: Admin,
    uri: Uri,
) -> Response {
    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);
    let sort = history_sort_selection(query_values(query, "sort"));

    let page_data = match app.history.load_history_page(page, size, sort).await {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    match history_page_dto(&page_data) {
        Ok(payload) => Json(payload).into_response(),
        Err(error) => {
            tracing::error!(?error, "history response mapping failed");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

fn history_page_dto(page: &HistoryPage) -> Result<PageDto<HistoryEventDto>> {
    let page_number = usize::try_from(page.page)?;
    let page_size = usize::try_from(page.size)?;
    let total_elements = usize::try_from(page.total_elements)?;
    let total_pages = usize::try_from(page.total_pages)?;
    let content = page
        .content
        .iter()
        .map(history_event_dto)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(PageDto::paged(
        content,
        page_number,
        page_size,
        total_elements,
        total_pages,
        page.sorted,
    ))
}

fn history_event_dto(event: &HistoryEvent) -> Result<HistoryEventDto> {
    Ok(HistoryEventDto {
        id: event.id.clone(),
        event_type: event.event_type.clone(),
        timestamp: KotlinLocalDateTime::parse(&event.timestamp)?,
        book_id: event.book_id.clone(),
        series_id: event.series_id.clone(),
        properties: event.properties.clone(),
    })
}

fn history_sort_selection(raw_sorts: Vec<String>) -> HistorySortSelection {
    if raw_sorts.is_empty() {
        return HistorySortSelection::default_timestamp_desc();
    }

    let sorts = raw_sorts
        .iter()
        .filter_map(|sort| parse_history_sort(sort))
        .collect();
    HistorySortSelection::from_requested_sorts(sorts)
}

fn parse_history_sort(sort: &str) -> Option<HistorySort> {
    let (property, direction) = sort
        .split_once(',')
        .map(|(property, direction)| (property.trim(), direction.trim()))
        .unwrap_or_else(|| (sort.trim(), "asc"));

    Some(HistorySort {
        property: match property {
            "type" => HistorySortProperty::Type,
            "bookId" => HistorySortProperty::BookId,
            "seriesId" => HistorySortProperty::SeriesId,
            "timestamp" => HistorySortProperty::Timestamp,
            _ => return None,
        },
        direction: if direction.eq_ignore_ascii_case("desc") {
            HistorySortDirection::Desc
        } else {
            HistorySortDirection::Asc
        },
    })
}

pub(crate) async fn delete_syncpoints_me(
    State(app): State<OperationalApiState>,
    Authenticated(current_user): Authenticated,
    uri: Uri,
) -> Response {
    let result = match syncpoint_delete_scope(uri.query().unwrap_or_default()) {
        SyncpointDeleteScope::All => {
            app.syncpoints
                .delete_syncpoints_by_user(user_id(&current_user))
                .await
        }
        SyncpointDeleteScope::ApiKeys(key_ids) => {
            app.syncpoints
                .delete_syncpoints_by_user_and_key_ids(user_id(&current_user), &key_ids)
                .await
        }
    };

    if result.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

fn syncpoint_delete_scope(query: &str) -> SyncpointDeleteScope {
    let key_ids = query_values(query, "key_id");
    match key_ids.as_slice() {
        [] => SyncpointDeleteScope::All,
        [single] => {
            let split_values = single
                .split(',')
                .map(|value| value.trim().to_string())
                .collect::<Vec<_>>();
            if split_values.is_empty() || (split_values.len() == 1 && single.is_empty()) {
                SyncpointDeleteScope::All
            } else {
                SyncpointDeleteScope::ApiKeys(split_values)
            }
        }
        _ => SyncpointDeleteScope::ApiKeys(key_ids),
    }
}

pub(crate) async fn get_oauth2_providers(State(app): State<OperationalApiState>) -> Response {
    let providers = app
        .operational
        .oauth2_clients
        .iter()
        .map(|provider| OAuth2ClientDto {
            name: provider.client_name.clone(),
            registration_id: provider.registration_id.clone(),
        })
        .collect::<Vec<_>>();

    Json(providers).into_response()
}

pub(crate) async fn delete_tasks(State(app): State<OperationalApiState>, _: Admin) -> Response {
    let deleted = match app.task_queue.queue.clear_unowned_tasks().await {
        Ok(deleted) => deleted,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(MessageDto {
                    message: format!("failed to delete tasks: {error:#}"),
                }),
            )
                .into_response();
        }
    };

    Json(deleted).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syncpoint_delete_scope_defaults_to_all_when_key_id_is_missing_or_empty() {
        assert_eq!(syncpoint_delete_scope(""), SyncpointDeleteScope::All);
        assert_eq!(syncpoint_delete_scope("foo=bar"), SyncpointDeleteScope::All);
        assert_eq!(syncpoint_delete_scope("key_id="), SyncpointDeleteScope::All);
    }

    #[test]
    fn syncpoint_delete_scope_keeps_repeated_key_ids_without_filtering_empty_values() {
        assert_eq!(
            syncpoint_delete_scope("key_id=key-1&key_id=key-2&key_id="),
            SyncpointDeleteScope::ApiKeys(vec![
                "key-1".to_string(),
                "key-2".to_string(),
                String::new(),
            ]),
        );
    }

    #[test]
    fn syncpoint_delete_scope_splits_single_comma_delimited_key_id_like_spring() {
        assert_eq!(
            syncpoint_delete_scope("key_id=key-1,key-2"),
            SyncpointDeleteScope::ApiKeys(vec!["key-1".to_string(), "key-2".to_string()]),
        );
        assert_eq!(
            syncpoint_delete_scope("key_id=key-1,+key-2"),
            SyncpointDeleteScope::ApiKeys(vec!["key-1".to_string(), "key-2".to_string()]),
        );
    }

    #[test]
    fn syncpoint_delete_scope_keeps_single_whitespace_only_key_id_as_empty_string() {
        assert_eq!(
            syncpoint_delete_scope("key_id=++"),
            SyncpointDeleteScope::ApiKeys(vec![String::new()]),
        );
    }

    #[test]
    fn syncpoint_delete_scope_keeps_repeated_key_ids_without_spring_single_value_splitting() {
        assert_eq!(
            syncpoint_delete_scope("key_id=&key_id=++"),
            SyncpointDeleteScope::ApiKeys(vec![String::new(), "  ".to_string()]),
        );
        assert_eq!(
            syncpoint_delete_scope("key_id=key-1,key-2&key_id=key-3"),
            SyncpointDeleteScope::ApiKeys(vec!["key-1,key-2".to_string(), "key-3".to_string()]),
        );
    }
}
