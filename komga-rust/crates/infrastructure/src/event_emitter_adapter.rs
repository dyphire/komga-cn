use komga_application::media_assets::metadata_writer::BookEventEmitter;
use komga_application::runtime_sse::register_runtime_sse_event;
use serde_json::json;

/// Adapter that emits book-changed SSE events to connected clients.
#[derive(Clone, Default)]
pub struct SseBookEventEmitter;

impl BookEventEmitter for SseBookEventEmitter {
    fn emit_book_changed(&self, book_id: &str, series_id: &str, library_id: &str) {
        register_runtime_sse_event(
            "BookChanged",
            json!({
                "bookId": book_id,
                "seriesId": series_id,
                "libraryId": library_id,
            }),
            false,
            None,
        );
    }
}
