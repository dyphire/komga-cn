use std::sync::Arc;

use komga_application::media_assets::BookEventEmitter;
use komga_application::runtime_sse::{RuntimeSseEvent, RuntimeSseEventSink};

/// Adapter that emits book-changed SSE events to connected clients.
#[derive(Clone)]
pub struct SseBookEventEmitter {
    runtime_events: Arc<dyn RuntimeSseEventSink>,
}

impl SseBookEventEmitter {
    pub fn new(runtime_events: Arc<dyn RuntimeSseEventSink>) -> Self {
        Self { runtime_events }
    }
}

impl BookEventEmitter for SseBookEventEmitter {
    fn emit_book_changed(&self, book_id: &str, series_id: &str, library_id: &str) {
        self.runtime_events.register(RuntimeSseEvent::BookChanged {
            book_id: book_id.to_string(),
            series_id: series_id.to_string(),
            library_id: library_id.to_string(),
        });
    }
}
