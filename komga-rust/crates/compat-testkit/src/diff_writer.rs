use crate::{ComparisonMode, DiffReport, NormalizedBody, NormalizedResponse, SerializableResponse};
use anyhow::Context;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

pub fn compare_responses(
    case_id: &str,
    java: &NormalizedResponse,
    rust: &NormalizedResponse,
    allowlist: &BTreeSet<String>,
    comparison_mode: ComparisonMode,
) -> DiffReport {
    let mut differences = Vec::new();
    let comparable_java_body = comparable_body(case_id, &java.body);
    let comparable_rust_body = comparable_body(case_id, &rust.body);

    if java.status != rust.status {
        differences.push(format!(
            "status mismatch: java={} rust={}",
            java.status, rust.status
        ));
    }

    for header in allowlist {
        let java_value = java.headers.get(header);
        let rust_value = rust.headers.get(header);
        if java_value != rust_value {
            differences.push(format!(
                "header mismatch for {header}: java={java_value:?} rust={rust_value:?}"
            ));
        }
    }

    if matches!(comparison_mode, ComparisonMode::Json | ComparisonMode::Xml)
        && comparable_java_body != comparable_rust_body
    {
        differences.push(format!(
            "body mismatch: java={} rust={}",
            render_body(&java.body),
            render_body(&rust.body)
        ));
    }

    DiffReport {
        case_id: case_id.to_string(),
        matches: differences.is_empty(),
        differences,
        java: SerializableResponse::from(java),
        rust: SerializableResponse::from(rust),
    }
}

fn comparable_body(case_id: &str, body: &NormalizedBody) -> NormalizedBody {
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

pub fn write_diff_report(output_dir: &Path, report: &DiffReport) -> anyhow::Result<()> {
    fs::create_dir_all(output_dir)
        .with_context(|| format!("failed to create diff output dir {}", output_dir.display()))?;
    let report_path = output_dir.join(format!("{}.json", report.case_id));
    let report_json =
        serde_json::to_string_pretty(report).context("failed to serialize diff report")?;
    fs::write(&report_path, report_json)
        .with_context(|| format!("failed to write diff report {}", report_path.display()))
}

fn render_body(body: &NormalizedBody) -> String {
    match body {
        NormalizedBody::Json(value) => value.to_string(),
        NormalizedBody::Text(value) => value.clone(),
        NormalizedBody::Empty => "<empty>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::compare_responses;
    use crate::{ComparisonMode, NormalizedBody, NormalizedResponse};
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn remember_me_diff_ignores_only_volatile_user_id() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = auth_response("java-volatile-id", "user@example.org");
        let rust = auth_response("rust-volatile-id", "user@example.org");

        let report = compare_responses(
            "P0-AUTH-REMEMBERME",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(
            report.matches,
            "remember-me diff should ignore volatile user ids only: {:?}",
            report.differences
        );
        assert!(report.differences.is_empty());
    }

    #[test]
    fn remember_me_diff_still_reports_other_auth_body_mismatches() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = auth_response("java-volatile-id", "user@example.org");
        let rust = auth_response("rust-volatile-id", "other@example.org");

        let report = compare_responses(
            "P0-AUTH-REMEMBERME",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(!report.matches);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.contains("body mismatch")),
            "expected body mismatch to remain meaningful: {:?}",
            report.differences
        );
    }

    #[test]
    fn api_key_auth_diff_ignores_only_volatile_user_id_for_both_header_casings() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);

        for case_id in ["P1-AUTH-APIKEY-UPPER", "P1-AUTH-APIKEY-LOWER"] {
            let java = auth_response("java-volatile-id", "user@example.org");
            let rust = auth_response("rust-volatile-id", "user@example.org");

            let report = compare_responses(case_id, &java, &rust, &allowlist, ComparisonMode::Json);

            assert!(
                report.matches,
                "{case_id} should ignore volatile user ids only: {:?}",
                report.differences
            );
            assert!(
                report.differences.is_empty(),
                "{case_id} should have no differences"
            );
        }
    }

    #[test]
    fn api_key_auth_diff_still_reports_other_auth_body_mismatches() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = auth_response("java-volatile-id", "user@example.org");
        let rust = auth_response("rust-volatile-id", "other@example.org");

        let report = compare_responses(
            "P1-AUTH-APIKEY-UPPER",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(!report.matches);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.contains("body mismatch")),
            "expected body mismatch to remain meaningful: {:?}",
            report.differences
        );
    }

    #[test]
    fn invalid_api_key_diff_ignores_only_volatile_unauthorized_timestamp() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = unauthorized_response("2026-03-21T11:43:17.616+00:00", "/api/v2/users/me");
        let rust = unauthorized_response("1970-01-01T00:00:00.000+00:00", "/api/v2/users/me");

        let report = compare_responses(
            "P1-AUTH-APIKEY-INVALID",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(
            report.matches,
            "invalid api key diff should ignore volatile timestamp only: {:?}",
            report.differences
        );
        assert!(report.differences.is_empty());
    }

    #[test]
    fn invalid_api_key_diff_still_reports_other_unauthorized_body_mismatches() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = unauthorized_response("2026-03-21T11:43:17.616+00:00", "/api/v2/users/me");
        let rust = unauthorized_response("1970-01-01T00:00:00.000+00:00", "/api/v2/other");

        let report = compare_responses(
            "P1-AUTH-APIKEY-INVALID",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(!report.matches);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.contains("body mismatch")),
            "expected body mismatch to remain meaningful: {:?}",
            report.differences
        );
    }

    #[test]
    fn read_progress_method_not_allowed_diff_ignores_only_volatile_timestamp() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = method_not_allowed_response(
            "2026-03-21T14:44:09.661+00:00",
            "/api/v1/books/book-1/read-progress",
        );
        let rust = method_not_allowed_response(
            "2026-03-21T14:44:09.800+00:00",
            "/api/v1/books/book-1/read-progress",
        );

        let report = compare_responses(
            "KOMGA-P0-BK-READ-PROGRESS-01",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(
            report.matches,
            "read-progress 405 diff should ignore volatile timestamp only: {:?}",
            report.differences
        );
        assert!(report.differences.is_empty());
    }

    #[test]
    fn read_progress_method_not_allowed_diff_still_reports_other_body_mismatches() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = method_not_allowed_response(
            "2026-03-21T14:44:09.661+00:00",
            "/api/v1/books/book-1/read-progress",
        );
        let rust = method_not_allowed_response(
            "2026-03-21T14:44:09.800+00:00",
            "/api/v1/books/book-2/read-progress",
        );

        let report = compare_responses(
            "KOMGA-P0-BK-READ-PROGRESS-01",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(!report.matches);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.contains("body mismatch")),
            "expected body mismatch to remain meaningful: {:?}",
            report.differences
        );
    }

    #[test]
    fn read_progress_delete_diff_ignores_only_volatile_timestamp() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = method_not_allowed_response(
            "2026-03-21T14:52:37.232+00:00",
            "/api/v1/books/book-1/read-progress",
        );
        let rust = method_not_allowed_response(
            "2026-03-21T14:52:37.363+00:00",
            "/api/v1/books/book-1/read-progress",
        );

        let report = compare_responses(
            "P1-BK-READ-PROGRESS-DELETE",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(
            report.matches,
            "read-progress delete 405 diff should ignore volatile timestamp only: {:?}",
            report.differences
        );
        assert!(report.differences.is_empty());
    }

    #[test]
    fn read_progress_missing_book_diff_ignores_only_volatile_timestamp() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = method_not_allowed_response(
            "2026-03-21T15:00:59.787+00:00",
            "/api/v1/books/book-missing/read-progress",
        );
        let rust = method_not_allowed_response(
            "2026-03-21T15:00:59.926+00:00",
            "/api/v1/books/book-missing/read-progress",
        );

        let report = compare_responses(
            "P1-BK-READ-PROGRESS-404",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(
            report.matches,
            "read-progress missing-book 405 diff should ignore volatile timestamp only: {:?}",
            report.differences
        );
        assert!(report.differences.is_empty());
    }

    #[test]
    fn read_progress_missing_book_diff_still_reports_other_body_mismatches() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = method_not_allowed_response(
            "2026-03-21T15:00:59.787+00:00",
            "/api/v1/books/book-missing/read-progress",
        );
        let rust = method_not_allowed_response(
            "2026-03-21T15:00:59.926+00:00",
            "/api/v1/books/book-other/read-progress",
        );

        let report = compare_responses(
            "P1-BK-READ-PROGRESS-404",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(!report.matches);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.contains("body mismatch")),
            "expected body mismatch to remain meaningful: {:?}",
            report.differences
        );
    }

    #[test]
    fn unrelated_method_not_allowed_diff_still_reports_timestamp_mismatches() {
        let allowlist = BTreeSet::from(["content-type".to_string()]);
        let java = method_not_allowed_response(
            "2026-03-21T14:52:37.232+00:00",
            "/api/v1/books/book-1/read-progress",
        );
        let rust = method_not_allowed_response(
            "2026-03-21T14:52:37.363+00:00",
            "/api/v1/books/book-1/read-progress",
        );

        let report = compare_responses(
            "UNLISTED-READ-PROGRESS-DELETE",
            &java,
            &rust,
            &allowlist,
            ComparisonMode::Json,
        );

        assert!(!report.matches);
        assert!(
            report
                .differences
                .iter()
                .any(|difference| difference.contains("body mismatch")),
            "expected timestamp mismatch to remain strict for unrelated cases: {:?}",
            report.differences
        );
    }

    fn auth_response(id: &str, email: &str) -> NormalizedResponse {
        NormalizedResponse {
            status: 200,
            headers: BTreeMap::from([(
                "content-type".to_string(),
                vec!["application/json".to_string()],
            )]),
            body: NormalizedBody::Json(serde_json::json!({
                "ageRestriction": null,
                "email": email,
                "id": id,
                "labelsAllow": [],
                "labelsExclude": [],
                "roles": ["FILE_DOWNLOAD", "PAGE_STREAMING", "USER"],
                "sharedAllLibraries": true,
                "sharedLibrariesIds": [],
            })),
        }
    }

    fn unauthorized_response(timestamp: &str, path: &str) -> NormalizedResponse {
        NormalizedResponse {
            status: 401,
            headers: BTreeMap::from([(
                "content-type".to_string(),
                vec!["application/json".to_string()],
            )]),
            body: NormalizedBody::Json(serde_json::json!({
                "error": "Unauthorized",
                "message": "Unauthorized",
                "path": path,
                "status": 401,
                "timestamp": timestamp,
            })),
        }
    }

    fn method_not_allowed_response(timestamp: &str, path: &str) -> NormalizedResponse {
        NormalizedResponse {
            status: 405,
            headers: BTreeMap::from([(
                "content-type".to_string(),
                vec!["application/json".to_string()],
            )]),
            body: NormalizedBody::Json(serde_json::json!({
                "error": "Method Not Allowed",
                "message": "Method 'GET' is not supported.",
                "path": path,
                "status": 405,
                "timestamp": timestamp,
                "trace": "org.springframework.web.HttpRequestMethodNotSupportedException: Request method 'GET' is not supported",
            })),
        }
    }
}
