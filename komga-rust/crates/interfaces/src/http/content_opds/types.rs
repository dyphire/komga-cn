use crate::http::discovery_auth::AgeRestrictionKind;

#[derive(Clone)]
pub(super) struct PersistedLibrary {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) last_modified: String,
}

pub(super) struct PersistedSeries {
    pub(super) id: String,
    pub(super) library_id: String,
    pub(super) title: String,
    pub(super) last_modified: String,
}

pub(super) struct PersistedSeriesBook {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) file_name: String,
    pub(super) media_type: String,
    pub(super) last_modified: String,
}

pub(super) struct PersistedReadlist {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) last_modified: String,
}

pub(super) struct PersistedReadlistBook {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) file_name: String,
    pub(super) media_type: String,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
}

pub(super) struct PersistedSeriesSearchResult {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) library_id: String,
}

pub(super) struct PersistedBookSearchResult {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) library_id: String,
}

pub(super) struct PersistedReadlistSearchResult {
    pub(super) id: String,
    pub(super) name: String,
}

pub(super) struct PersistedCollectionSearchResult {
    pub(super) id: String,
    pub(super) name: String,
}

pub(super) struct PersistedBookFeedItem {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) file_name: String,
    pub(super) media_type: String,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
}

pub(super) struct PersistedCollection {
    pub(super) id: String,
    pub(super) name: String,
}

#[derive(Clone, Debug, Default)]
pub(super) struct OpdsRestrictions {
    pub(super) age: Option<u16>,
    pub(super) age_restriction: Option<AgeRestrictionKind>,
    pub(super) labels_allow: Vec<String>,
    pub(super) labels_exclude: Vec<String>,
}
