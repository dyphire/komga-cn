use axum::Json;
use axum::extract::Extension;
use axum::http::Uri;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

use crate::http::identity_access::auth::{
    require_admin, require_auth, resolved_auth_user, user_id,
};
use crate::operational_settings_access::operations as operations_access;

use super::super::super::OperationalState;
use super::{query_value, query_values};

#[derive(Debug, Eq, PartialEq)]
enum SyncpointDeleteScope {
    All,
    ApiKeys(Vec<String>),
}

pub(crate) async fn get_history(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let query = uri.query().unwrap_or_default();
    let page = query_value(query, "page")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    let size = query_value(query, "size")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(20);

    let page_data = match operations_access::load_history_page(
        state.runtime.database_file.as_path(),
        page,
        size,
    )
    .await
    {
        Ok(page_data) => page_data,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    Json(page_data).into_response()
}

pub(crate) async fn delete_syncpoints_me(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
    uri: Uri,
) -> Response {
    if let Some(response) = require_auth(&headers) {
        return response;
    }

    let Some(current_user) = resolved_auth_user(&headers) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };

    let result = match syncpoint_delete_scope(uri.query().unwrap_or_default()) {
        SyncpointDeleteScope::All => {
            operations_access::delete_syncpoints_by_user(
                state.runtime.database_file.as_path(),
                user_id(&current_user),
            )
            .await
        }
        SyncpointDeleteScope::ApiKeys(key_ids) => {
            operations_access::delete_syncpoints_by_user_and_key_ids(
                state.runtime.database_file.as_path(),
                user_id(&current_user),
                &key_ids,
            )
            .await
        }
    };

    if result.is_err() {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }

    StatusCode::NO_CONTENT.into_response()
}

fn syncpoint_delete_scope(query: &str) -> SyncpointDeleteScope {
    let key_ids = query_values(query, "key_id")
        .into_iter()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    if key_ids.is_empty() {
        SyncpointDeleteScope::All
    } else {
        SyncpointDeleteScope::ApiKeys(key_ids)
    }
}

pub(crate) async fn get_oauth2_providers(
    Extension(state): Extension<OperationalState>,
) -> Response {
    let providers = state
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

pub(crate) async fn delete_tasks(
    Extension(state): Extension<OperationalState>,
    headers: HeaderMap,
) -> Response {
    if let Some(response) = require_admin(&headers) {
        return response;
    }

    let deleted = (state.clear_unowned_tasks)();

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
        assert_eq!(
            syncpoint_delete_scope("key_id=&key_id=   "),
            SyncpointDeleteScope::All
        );
    }

    #[test]
    fn syncpoint_delete_scope_keeps_all_non_empty_key_ids() {
        assert_eq!(
            syncpoint_delete_scope("key_id=key-1&key_id=key-2&key_id="),
            SyncpointDeleteScope::ApiKeys(vec!["key-1".to_string(), "key-2".to_string()]),
        );
    }
}
