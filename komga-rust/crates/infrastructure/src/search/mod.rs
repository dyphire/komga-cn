mod analyzers;
mod documents;
pub(crate) mod engine;
mod lifecycle;
mod sync_adapter;

pub use analyzers::search_analyzer_version;
pub use engine::rebuild_index_from_database;
pub use lifecycle::{
    SearchEntityType, SearchIndexLifecycle, SearchStartupLifecycle, decide_startup_lifecycle,
    prepare_for_rebuild,
};
pub use sync_adapter::SearchSyncAdapter;
