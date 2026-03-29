#[derive(Clone, Debug, Eq, PartialEq)]
pub enum UnsupportedDiscoverySemantics {
    UnsupportedSeriesSort(String),
    UnsupportedBookSort(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DiscoveryError {
    UnsupportedSemantics(UnsupportedDiscoverySemantics),
    InvalidSemantics(String),
    Persistence(String),
}
