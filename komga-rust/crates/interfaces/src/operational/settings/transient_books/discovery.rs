use crate::state::OperationalApiState;

pub(super) async fn infer_transient_series_and_number(
    app: &OperationalApiState,
    transient_name: &str,
) -> (Option<String>, Option<f64>) {
    app.transient_books
        .infer_transient_series_and_number(transient_name)
        .await
}

pub(super) fn list_transient_book_entries(
    app: &OperationalApiState,
    root: std::path::PathBuf,
) -> Vec<serde_json::Value> {
    app.transient_books.list_transient_book_entries(&root)
}
