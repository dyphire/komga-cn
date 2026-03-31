#![allow(clippy::result_large_err)]

use super::*;
use komga_application::identity_access::random_uuid_like;

pub(super) fn register_discovery_principal(
    auth_state: &DiscoveryAuthState,
    payload: &serde_json::Value,
    token: &str,
) {
    if let Some(principal) = principal_from_user_payload(payload) {
        auth_state.register_session_principal(token, principal);
    }
}

#[derive(Clone, Debug)]
pub(super) struct SharedLibrariesPatch {
    pub(super) all: bool,
    pub(super) library_ids: Vec<String>,
}

#[derive(Clone, Debug)]
pub(super) struct AgeRestrictionPatch {
    pub(super) age: i64,
    pub(super) allow_only: bool,
}

pub(super) fn bad_request(message: &str) -> Response {
    (StatusCode::BAD_REQUEST, Json(json!({ "message": message }))).into_response()
}

pub(super) fn generated_user_id() -> String {
    random_uuid_like()
}

pub(super) fn looks_like_kotlin_user_email(value: &str) -> bool {
    let mut parts = value.split('@');
    let Some(local) = parts.next() else {
        return false;
    };
    let Some(domain) = parts.next() else {
        return false;
    };
    if parts.next().is_some() {
        return false;
    }

    if local.is_empty() || domain.is_empty() {
        return false;
    }

    let mut domain_segments = domain.split('.');
    let has_all_non_empty_segments = domain_segments.all(|segment| !segment.is_empty());
    has_all_non_empty_segments && domain.contains('.')
}

pub(super) fn parse_roles_array(value: Option<&Value>) -> Result<Vec<String>, Response> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    if value.is_null() {
        return Ok(Vec::new());
    }

    let Some(values) = value.as_array() else {
        return Err(bad_request("roles must be an array of strings"));
    };

    let mut roles = BTreeSet::new();
    for value in values {
        let Some(role) = value.as_str() else {
            return Err(bad_request("roles must be an array of strings"));
        };
        if matches!(
            role,
            "ADMIN" | "FILE_DOWNLOAD" | "PAGE_STREAMING" | "KOBO_SYNC" | "KOREADER_SYNC"
        ) {
            roles.insert(role.to_string());
        }
    }
    Ok(roles.into_iter().collect())
}

pub(super) fn parse_string_set_optional(
    value: Option<&Value>,
) -> Result<Option<Vec<String>>, Response> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(Some(Vec::new()));
    }

    let Some(values) = value.as_array() else {
        return Err(bad_request("labels must be an array of strings"));
    };

    let mut labels = BTreeSet::new();
    for value in values {
        let Some(label) = value.as_str() else {
            return Err(bad_request("labels must be an array of strings"));
        };
        let label = label.trim();
        if label.is_empty() {
            continue;
        }
        labels.insert(label.to_string());
    }

    Ok(Some(labels.into_iter().collect()))
}

pub(super) fn parse_age_restriction_optional(
    value: Option<&Value>,
) -> Result<Option<AgeRestrictionPatch>, Response> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }

    let Some(object) = value.as_object() else {
        return Err(bad_request("ageRestriction must be an object"));
    };

    let Some(age) = object.get("age").and_then(Value::as_i64) else {
        return Err(bad_request("ageRestriction.age must be an integer"));
    };
    if age < 0 {
        return Err(bad_request("ageRestriction.age must be >= 0"));
    }

    let Some(restriction) = object.get("restriction").and_then(Value::as_str) else {
        return Err(bad_request(
            "ageRestriction.restriction must be ALLOW_ONLY, EXCLUDE, or NONE",
        ));
    };

    match restriction {
        "ALLOW_ONLY" => Ok(Some(AgeRestrictionPatch {
            age,
            allow_only: true,
        })),
        "EXCLUDE" => Ok(Some(AgeRestrictionPatch {
            age,
            allow_only: false,
        })),
        "NONE" => Ok(None),
        _ => Err(bad_request(
            "ageRestriction.restriction must be ALLOW_ONLY, EXCLUDE, or NONE",
        )),
    }
}

pub(super) fn parse_shared_libraries_patch(
    value: Option<&Value>,
) -> Result<SharedLibrariesPatch, Response> {
    let Some(value) = value else {
        return Err(bad_request("sharedLibraries is required"));
    };
    let Some(object) = value.as_object() else {
        return Err(bad_request("sharedLibraries must be an object"));
    };

    let Some(all) = object.get("all").and_then(Value::as_bool) else {
        return Err(bad_request("sharedLibraries.all must be a boolean"));
    };

    let library_ids = if all {
        Vec::new()
    } else {
        let Some(ids) = object.get("libraryIds").and_then(Value::as_array) else {
            return Err(bad_request(
                "sharedLibraries.libraryIds must be an array of strings",
            ));
        };

        let mut normalized = BTreeSet::new();
        for value in ids {
            let Some(library_id) = value.as_str() else {
                return Err(bad_request(
                    "sharedLibraries.libraryIds must be an array of strings",
                ));
            };
            let library_id = library_id.trim();
            if library_id.is_empty() {
                continue;
            }
            normalized.insert(library_id.to_string());
        }
        normalized.into_iter().collect::<Vec<_>>()
    };

    Ok(SharedLibrariesPatch { all, library_ids })
}

pub(super) fn parse_shared_libraries_create(
    value: Option<&Value>,
) -> Result<SharedLibrariesPatch, Response> {
    let Some(value) = value else {
        return Ok(SharedLibrariesPatch {
            all: true,
            library_ids: Vec::new(),
        });
    };
    parse_shared_libraries_patch(Some(value))
}

pub(super) fn password_from_request(body: &Value) -> Option<&str> {
    body.get("password")?
        .as_str()
        .filter(|password| !password.trim().is_empty())
}

pub(super) fn api_key_comment_from_request(body: &Value) -> Option<String> {
    let comment = body.get("comment")?.as_str()?.trim();
    if comment.is_empty() {
        None
    } else {
        Some(comment.to_string())
    }
}

pub(super) async fn authenticated_user(
    headers: &HeaderMap,
    auth_db: &AuthDatabaseState,
) -> Option<AuthUser> {
    match persisted_api_key_user(headers, &auth_db.database_file)
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => return Some(*user),
        AuthOutcome::Invalid => return None,
        AuthOutcome::Missing => {}
    }

    if let Some(user) = auth_token_user(headers) {
        return Some(user);
    }

    match persisted_basic_user(headers, &auth_db.database_file)
        .await
        .unwrap_or(AuthOutcome::Missing)
    {
        AuthOutcome::Valid(user) => Some(*user),
        AuthOutcome::Invalid | AuthOutcome::Missing => None,
    }
}

pub(super) fn authentication_activity_page_payload(
    rows: Vec<PersistedAuthenticationActivity>,
    unpaged: bool,
) -> Value {
    let content = rows
        .iter()
        .map(authentication_activity_payload)
        .collect::<Vec<_>>();
    let number_of_elements = content.len() as u64;
    let page_size = if unpaged { number_of_elements } else { 20 };
    let total_pages = if unpaged {
        1
    } else if number_of_elements == 0 {
        0
    } else {
        number_of_elements.div_ceil(page_size)
    };

    json!({
        "content": content,
        "pageable": {
            "pageNumber": 0,
            "pageSize": page_size,
            "sort": {
                "empty": false,
                "sorted": true,
                "unsorted": false
            },
            "offset": 0,
            "paged": !unpaged,
            "unpaged": unpaged
        },
        "last": true,
        "totalElements": number_of_elements,
        "totalPages": total_pages,
        "first": true,
        "size": page_size,
        "number": 0,
        "sort": {
            "empty": false,
            "sorted": true,
            "unsorted": false
        },
        "numberOfElements": number_of_elements,
        "empty": number_of_elements == 0
    })
}

pub(super) fn authentication_activity_payload(activity: &PersistedAuthenticationActivity) -> Value {
    json!({
        "userId": activity.user_id(),
        "email": activity.email(),
        "ip": activity.ip(),
        "userAgent": activity.user_agent(),
        "success": activity.success(),
        "error": activity.error(),
        "dateTime": sqlite_datetime_to_utc(activity.date_time()),
        "source": activity.source(),
        "apiKeyId": activity.api_key_id(),
        "apiKeyComment": activity.api_key_comment(),
    })
}

fn sqlite_datetime_to_utc(value: &str) -> String {
    if value.ends_with('Z') || value.contains('T') {
        value.to_string()
    } else if let Some((date, time)) = value.split_once(' ') {
        format!("{date}T{time}Z")
    } else {
        value.to_string()
    }
}

pub(super) fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|pair| {
        let mut parts = pair.splitn(2, '=');
        let name = parts.next().unwrap_or_default();
        if name != key {
            return None;
        }
        Some(parts.next().unwrap_or_default())
    })
}

pub(super) fn query_bool(query: &str, key: &str) -> bool {
    query_value(query, key)
        .map(|value| value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
