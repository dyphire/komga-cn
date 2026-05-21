use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::http::Uri;
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::identity_access::auth::{Admin, Authenticated, user_id};
use crate::state::OperationalApiState;

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
    let sorts = query_values(query, "sort");

    let page_data = match app
        .operational_settings
        .load_history_page(page, size, sorts)
        .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn delete_syncpoints_me(
    State(app): State<OperationalApiState>,
    Authenticated(current_user): Authenticated,
    uri: Uri,
) -> Response {
    let result = match syncpoint_delete_scope(uri.query().unwrap_or_default()) {
        SyncpointDeleteScope::All => {
            app.operational_settings
                .delete_syncpoints_by_user(user_id(&current_user))
                .await
        }
        SyncpointDeleteScope::ApiKeys(key_ids) => {
            app.operational_settings
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
        .map(|provider| {
            json!({
                "name": provider.client_name,
                "registrationId": provider.registration_id,
            })
        })
        .collect::<Vec<_>>();

    Json(providers).into_response()
}

pub(crate) async fn delete_tasks(State(app): State<OperationalApiState>, _: Admin) -> Response {
    let deleted = app.task_queue.queue.clear_unowned_tasks().await;

    Json(json!(deleted)).into_response()
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
