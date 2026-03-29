#[path = "read_models/adapter.rs"]
mod adapter;
#[path = "read_models/filters.rs"]
mod filters;
#[path = "read_models/libraries.rs"]
mod libraries;
#[path = "read_models/queries.rs"]
mod queries;
#[path = "read_models/rows.rs"]
mod rows;
#[path = "read_models/runtime_sqlx.rs"]
mod runtime_sqlx;

pub use adapter::SqliteDiscoveryAdapter;
pub use libraries::{PersistedLibraryReadModel, get_persisted_library, list_persisted_libraries};
pub(crate) use rows::{
    BookRow, CollectionRow, LibraryRow, ReadListRow, ReadProgressRow, SeriesRow,
};
pub use runtime_sqlx::{SqlxRuntimeDiscoveryAdapter, SqlxRuntimeDiscoveryStore};
