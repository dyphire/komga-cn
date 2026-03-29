use crate::NormalizedBody;

pub(super) fn comparable_body(case_id: &str, body: &NormalizedBody) -> NormalizedBody {
    if should_ignore_volatile_auth_user_id(case_id) {
        return normalize_auth_user_id_body(body);
    }

    if should_ignore_volatile_unauthorized_timestamp(case_id) {
        return normalize_unauthorized_timestamp_body(body);
    }

    if should_ignore_volatile_read_progress_method_not_allowed_timestamp(case_id) {
        return normalize_read_progress_method_not_allowed_timestamp_body(body);
    }

    body.clone()
}

fn should_ignore_volatile_auth_user_id(case_id: &str) -> bool {
    matches!(
        case_id,
        "P0-AUTH-REMEMBERME" | "P1-AUTH-APIKEY-UPPER" | "P1-AUTH-APIKEY-LOWER"
    )
}

fn should_ignore_volatile_unauthorized_timestamp(case_id: &str) -> bool {
    matches!(case_id, "P1-AUTH-APIKEY-INVALID")
}

fn should_ignore_volatile_read_progress_method_not_allowed_timestamp(case_id: &str) -> bool {
    matches!(
        case_id,
        "KOMGA-P0-BK-READ-PROGRESS-01" | "P1-BK-READ-PROGRESS-DELETE" | "P1-BK-READ-PROGRESS-404"
    )
}

fn normalize_auth_user_id_body(body: &NormalizedBody) -> NormalizedBody {
    match body {
        NormalizedBody::Json(value) => NormalizedBody::Json(remove_volatile_auth_user_id(value)),
        _ => body.clone(),
    }
}

fn normalize_unauthorized_timestamp_body(body: &NormalizedBody) -> NormalizedBody {
    match body {
        NormalizedBody::Json(value) => {
            NormalizedBody::Json(remove_volatile_unauthorized_timestamp(value))
        }
        _ => body.clone(),
    }
}

fn normalize_read_progress_method_not_allowed_timestamp_body(
    body: &NormalizedBody,
) -> NormalizedBody {
    match body {
        NormalizedBody::Json(value) => NormalizedBody::Json(
            remove_volatile_read_progress_method_not_allowed_timestamp(value),
        ),
        _ => body.clone(),
    }
}

fn remove_volatile_auth_user_id(value: &serde_json::Value) -> serde_json::Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };

    let is_auth_user_shape = object.contains_key("email")
        && object.contains_key("roles")
        && object.contains_key("sharedAllLibraries")
        && object.contains_key("id");

    if !is_auth_user_shape {
        return value.clone();
    }

    let mut normalized = object.clone();
    normalized.remove("id");
    serde_json::Value::Object(normalized)
}

fn remove_volatile_unauthorized_timestamp(value: &serde_json::Value) -> serde_json::Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };

    let is_unauthorized_shape = object.get("error")
        == Some(&serde_json::Value::String("Unauthorized".to_string()))
        && object.get("message") == Some(&serde_json::Value::String("Unauthorized".to_string()))
        && object.get("status") == Some(&serde_json::Value::Number(401.into()))
        && object.contains_key("path")
        && object.contains_key("timestamp");

    if !is_unauthorized_shape {
        return value.clone();
    }

    let mut normalized = object.clone();
    normalized.remove("timestamp");
    serde_json::Value::Object(normalized)
}

fn remove_volatile_read_progress_method_not_allowed_timestamp(
    value: &serde_json::Value,
) -> serde_json::Value {
    let Some(object) = value.as_object() else {
        return value.clone();
    };

    let is_read_progress_method_not_allowed_shape = object.get("error")
        == Some(&serde_json::Value::String("Method Not Allowed".to_string()))
        && object.get("message")
            == Some(&serde_json::Value::String(
                "Method 'GET' is not supported.".to_string(),
            ))
        && object.get("status") == Some(&serde_json::Value::Number(405.into()))
        && matches!(
            object.get("path"),
            Some(serde_json::Value::String(path))
                if path == "/api/v1/books/book-1/read-progress"
                    || path == "/api/v1/books/book-missing/read-progress"
        )
        && object.contains_key("timestamp")
        && object.contains_key("trace");

    if !is_read_progress_method_not_allowed_shape {
        return value.clone();
    }

    let mut normalized = object.clone();
    normalized.remove("timestamp");
    serde_json::Value::Object(normalized)
}
