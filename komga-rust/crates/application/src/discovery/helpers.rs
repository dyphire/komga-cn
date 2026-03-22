use komga_domain::discovery::{DiscoveryError, NonNativeRequestShape};

pub(in crate::discovery) fn unsupported_book_filter(filter: impl Into<String>) -> DiscoveryError {
    DiscoveryError::NonNativeRequestShape(NonNativeRequestShape::UnsupportedBookFilter(
        filter.into(),
    ))
}

pub(in crate::discovery) fn unsupported_book_sort(sort: impl Into<String>) -> DiscoveryError {
    DiscoveryError::NonNativeRequestShape(NonNativeRequestShape::UnsupportedBookSort(sort.into()))
}
