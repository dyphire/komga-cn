use axum::http::{header, HeaderMap};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContentRestrictions {
    pub age: Option<u16>,
    pub age_restriction: Option<AgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryPrincipal {
    pub user_id: String,
    pub roles: Vec<String>,
    pub shared_all_libraries: bool,
    pub shared_library_ids: Vec<String>,
    pub restrictions: ContentRestrictions,
}

impl DiscoveryPrincipal {
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|role| role == "ADMIN")
    }

    pub fn can_access_all_libraries(&self) -> bool {
        self.shared_all_libraries || self.is_admin()
    }

    pub fn authorized_library_ids(
        &self,
        requested_library_ids: Option<&[String]>,
    ) -> Option<Vec<String>> {
        match (self.can_access_all_libraries(), requested_library_ids) {
            (false, Some(requested)) => Some(intersection(requested, &self.shared_library_ids)),
            (false, None) => Some(self.shared_library_ids.clone()),
            (true, Some(requested)) => Some(requested.to_vec()),
            (true, None) => None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRestrictions {
    pub age: Option<u16>,
    pub age_restriction: Option<AgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryQueryContext {
    pub user_id: Option<String>,
    pub is_admin: bool,
    pub authorized_library_ids: Option<Vec<String>>,
    pub restrictions: Option<QueryRestrictions>,
}

pub fn to_query_context(
    principal: &DiscoveryPrincipal,
    requested_library_ids: Option<&[String]>,
) -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: Some(principal.user_id.clone()),
        is_admin: principal.is_admin(),
        authorized_library_ids: principal.authorized_library_ids(requested_library_ids),
        restrictions: restrictions_for_query(&principal.restrictions),
    }
}

#[derive(Clone, Default)]
pub struct DiscoveryAuthState {
    principals_by_session_token: Arc<Mutex<HashMap<String, DiscoveryPrincipal>>>,
}

impl DiscoveryAuthState {
    pub fn register_session_principal(&self, session_token: &str, principal: DiscoveryPrincipal) {
        let token = session_token.trim();
        if token.is_empty() {
            return;
        }
        let mut sessions = self
            .principals_by_session_token
            .lock()
            .expect("discovery auth session store lock should not be poisoned");
        sessions.insert(token.to_string(), principal);
    }

    pub fn resolve_query_context(
        &self,
        headers: &HeaderMap,
        requested_library_ids: Option<&[String]>,
    ) -> Option<DiscoveryQueryContext> {
        let session_token = session_token_from_headers(headers)?;
        let principal = self
            .principals_by_session_token
            .lock()
            .expect("discovery auth session store lock should not be poisoned")
            .get(&session_token)
            .cloned()?;

        Some(to_query_context(&principal, requested_library_ids))
    }
}

pub fn principal_from_user_payload(payload: &Value) -> Option<DiscoveryPrincipal> {
    let user_id = payload.get("id")?.as_str()?.trim().to_string();
    if user_id.is_empty() {
        return None;
    }

    let roles = payload
        .get("roles")
        .and_then(Value::as_array)
        .map(|roles| {
            roles
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|role| !role.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let shared_all_libraries = payload
        .get("sharedAllLibraries")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let shared_library_ids = payload
        .get("sharedLibrariesIds")
        .and_then(Value::as_array)
        .map(|library_ids| {
            library_ids
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|library_id| !library_id.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let age = payload
        .get("ageRestriction")
        .and_then(|value| value.get("age"))
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let age_restriction = payload
        .get("ageRestriction")
        .and_then(|value| value.get("restriction"))
        .and_then(Value::as_str)
        .and_then(|value| match value.trim().to_ascii_uppercase().as_str() {
            "ALLOW_ONLY" => Some(AgeRestrictionKind::AllowOnly),
            "EXCLUDE" => Some(AgeRestrictionKind::Exclude),
            _ => None,
        });

    let labels_allow = payload
        .get("labelsAllow")
        .and_then(Value::as_array)
        .map(|labels| normalized_labels(labels))
        .unwrap_or_default();
    let labels_exclude = payload
        .get("labelsExclude")
        .and_then(Value::as_array)
        .map(|labels| normalized_labels(labels))
        .unwrap_or_default();

    Some(DiscoveryPrincipal {
        user_id,
        roles,
        shared_all_libraries,
        shared_library_ids,
        restrictions: ContentRestrictions {
            age,
            age_restriction,
            labels_allow,
            labels_exclude,
        },
    })
}

fn intersection(requested: &[String], authorized: &[String]) -> Vec<String> {
    requested
        .iter()
        .filter(|candidate| authorized.contains(*candidate))
        .cloned()
        .collect::<Vec<_>>()
}

fn restrictions_for_query(restrictions: &ContentRestrictions) -> Option<QueryRestrictions> {
    let has_restrictions = restrictions.age.is_some()
        || restrictions.age_restriction.is_some()
        || !restrictions.labels_allow.is_empty()
        || !restrictions.labels_exclude.is_empty();

    has_restrictions.then(|| QueryRestrictions {
        age: restrictions.age,
        age_restriction: restrictions.age_restriction,
        labels_allow: restrictions.labels_allow.clone(),
        labels_exclude: restrictions.labels_exclude.clone(),
    })
}

fn normalized_labels(labels: &[Value]) -> Vec<String> {
    labels
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(|label| label.to_ascii_lowercase())
        .collect::<Vec<_>>()
}

fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    x_auth_token(headers).or_else(|| session_cookie_token(headers))
}

fn x_auth_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get("X-Auth-Token")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn session_cookie_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get(header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookies| {
            cookies
                .split(';')
                .map(str::trim)
                .find_map(|cookie| cookie.strip_prefix("KOMGA-SESSION="))
        })
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(str::to_string)
}
