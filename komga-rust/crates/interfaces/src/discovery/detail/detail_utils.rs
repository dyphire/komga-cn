use axum::http::StatusCode;
use axum::response::Response;

use crate::helpers::spring_error_response;

pub(super) fn parse_group_concat_values(raw: &str) -> Vec<String> {
    const SEPARATOR: char = '\u{1e}';

    if raw.trim().is_empty() {
        return vec![];
    }

    raw.split(SEPARATOR)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn internal_error_response(error: impl std::fmt::Display + std::fmt::Debug) -> Response {
    tracing::error!(?error, "internal discovery detail error");
    spring_error_response(StatusCode::INTERNAL_SERVER_ERROR, format!("{error:#}"))
}
