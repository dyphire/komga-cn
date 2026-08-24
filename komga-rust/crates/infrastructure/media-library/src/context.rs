use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use komga_application::runtime_sse::RuntimeSseEventSink;
use sqlx::SqlitePool;

use komga_infrastructure_base::DatabaseHandle;

#[derive(Clone)]
pub struct MediaLibraryJobContext {
    main_db: DatabaseHandle,
    owns_main_database: bool,
    owns_filesystem_scan_output: bool,
    runtime_events: Arc<dyn RuntimeSseEventSink>,
    runtime_state: Arc<MediaLibraryRuntimeState>,
}

#[derive(Default)]
struct MediaLibraryRuntimeState {
    failed_book_conversions: Mutex<HashSet<String>>,
    skipped_extension_repairs: Mutex<HashSet<String>>,
}

pub struct MediaLibraryDatabaseContext<'a> {
    context: &'a MediaLibraryJobContext,
}

pub struct MediaLibraryFilesystemContext<'a> {
    context: &'a MediaLibraryJobContext,
}

impl MediaLibraryJobContext {
    pub fn new(
        main_db: DatabaseHandle,
        owns_main_database: bool,
        owns_filesystem_scan_output: bool,
        runtime_events: Arc<dyn RuntimeSseEventSink>,
    ) -> Self {
        Self {
            main_db,
            owns_main_database,
            owns_filesystem_scan_output,
            runtime_events,
            runtime_state: Arc::new(MediaLibraryRuntimeState::default()),
        }
    }

    pub fn database(&self) -> MediaLibraryDatabaseContext<'_> {
        MediaLibraryDatabaseContext { context: self }
    }

    pub fn filesystem(&self) -> MediaLibraryFilesystemContext<'_> {
        MediaLibraryFilesystemContext { context: self }
    }

    pub fn runtime_events(&self) -> &dyn RuntimeSseEventSink {
        self.runtime_events.as_ref()
    }

    pub fn runtime_events_arc(&self) -> Arc<dyn RuntimeSseEventSink> {
        self.runtime_events.clone()
    }

    pub fn book_conversion_failed_before(&self, book_id: &str) -> bool {
        self.runtime_state
            .failed_book_conversions
            .lock()
            .expect("failed book conversion state lock should not be poisoned")
            .contains(book_id)
    }

    pub fn mark_book_conversion_failed(&self, book_id: &str) {
        self.runtime_state
            .failed_book_conversions
            .lock()
            .expect("failed book conversion state lock should not be poisoned")
            .insert(book_id.to_string());
    }

    pub fn extension_repair_was_skipped(&self, book_id: &str) -> bool {
        self.runtime_state
            .skipped_extension_repairs
            .lock()
            .expect("skipped extension repair state lock should not be poisoned")
            .contains(book_id)
    }

    pub fn mark_extension_repair_skipped(&self, book_id: &str) {
        self.runtime_state
            .skipped_extension_repairs
            .lock()
            .expect("skipped extension repair state lock should not be poisoned")
            .insert(book_id.to_string());
    }
}

impl MediaLibraryDatabaseContext<'_> {
    pub fn read_pool(&self) -> &SqlitePool {
        self.context.main_db.read_pool()
    }

    pub fn write_pool(&self) -> &SqlitePool {
        self.context.main_db.write_pool()
    }

    pub fn owns_main_database(&self) -> bool {
        self.context.owns_main_database
    }
}

impl MediaLibraryFilesystemContext<'_> {
    pub fn owns_filesystem_scan_output(&self) -> bool {
        self.context.owns_filesystem_scan_output
    }
}
