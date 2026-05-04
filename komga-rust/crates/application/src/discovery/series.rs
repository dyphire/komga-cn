#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesDetailQuery {
    pub series_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesCollectionsQuery {
    pub series_id: String,
}
