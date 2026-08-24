mod books;
mod collections;
pub(crate) mod deletion;
mod detail_adapter;
mod libraries;
mod persisted;
mod query_values;
mod readlists;
mod records;
mod series;
mod set_persistence;
mod visibility;

pub use detail_adapter::DiscoveryDetailAccess;
pub use libraries::LibraryCatalogAccess;
pub use persisted::{DiscoveryQuerySupportAccess, SqliteDiscoveryBrowseService};
