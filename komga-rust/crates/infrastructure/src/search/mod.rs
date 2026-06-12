pub(crate) mod analyzer_profiles;
mod documents;
pub(crate) mod engine;
pub(crate) mod index_lifecycle;

pub use analyzer_profiles::search_analyzer_version;
pub use engine::rebuild_index_from_database;
pub use index_lifecycle::{
    SearchEntityType, SearchIndexLifecycle, SearchStartupLifecycle, decide_startup_lifecycle,
    prepare_for_rebuild,
};
