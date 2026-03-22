#[path = "diff_writer/compare.rs"]
mod compare;
#[path = "diff_writer/normalize.rs"]
mod normalize;
#[path = "diff_writer/report.rs"]
mod report;

pub use compare::compare_responses;
pub use report::write_diff_report;

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
