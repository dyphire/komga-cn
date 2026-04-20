use crate::http::state::HttpAppState;

pub(super) async fn infer_transient_series_and_number(
    app: &HttpAppState,
    transient_name: &str,
) -> (Option<String>, Option<f64>) {
    app.services
        .operational_settings
        .infer_transient_series_and_number(
            app.operational.runtime.database_file.clone(),
            transient_name.to_string(),
        )
        .await
}

pub(super) fn list_transient_book_entries(
    app: &HttpAppState,
    root: std::path::PathBuf,
) -> Vec<serde_json::Value> {
    app.services
        .operational_settings
        .list_transient_book_entries(root)
}
