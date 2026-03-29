use crate::common_ids::UserId;

use super::{BookFilter, DiscoveryError, DiscoverySavedSearch, SeriesFilter};

pub trait DiscoveryWritePort {
    fn save_series_filter(
        &self,
        owner_id: &UserId,
        name: &str,
        filter: &SeriesFilter,
    ) -> Result<(), DiscoveryError>;

    fn save_book_filter(
        &self,
        owner_id: &UserId,
        name: &str,
        filter: &BookFilter,
    ) -> Result<(), DiscoveryError>;
}

pub trait DiscoverySavedSearchWritePort {
    fn save(
        &self,
        owner_id: &UserId,
        saved_search: &DiscoverySavedSearch,
    ) -> Result<(), DiscoveryError>;

    fn delete(&self, owner_id: &UserId, name: &str) -> Result<(), DiscoveryError>;
}
