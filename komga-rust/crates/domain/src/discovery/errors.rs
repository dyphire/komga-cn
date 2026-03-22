#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NonNativeRequestShape {
    UnsupportedSeriesSort(String),
    UnsupportedSeriesFilter(String),
    UnsupportedBookSort(String),
    UnsupportedBookFilter(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    NonNativeRequestShape(NonNativeRequestShape),
    InvalidRequest(String),
    Persistence(String),
}
