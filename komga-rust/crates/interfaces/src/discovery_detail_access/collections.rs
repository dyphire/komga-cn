#[derive(Clone)]
pub struct PersistedCollectionRecord {
    pub id: String,
    pub name: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

pub struct PersistedSeriesRestrictionRecord {
    pub age_rating: Option<u16>,
    pub labels: Vec<String>,
}
