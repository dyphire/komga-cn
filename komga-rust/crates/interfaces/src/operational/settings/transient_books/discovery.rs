use crate::state::OperationalApiState;

pub(super) async fn infer_transient_series_and_number(
    app: &OperationalApiState,
    transient_name: &str,
) -> (Option<String>, Option<f64>) {
    app.operational_settings
        .infer_transient_series_and_number(transient_name)
        .await
}

pub(super) fn list_transient_book_entries(
    app: &OperationalApiState,
    root: std::path::PathBuf,
) -> Vec<serde_json::Value> {
    app.operational_settings.list_transient_book_entries(&root)
}
