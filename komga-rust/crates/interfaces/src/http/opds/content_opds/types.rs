use crate::http::discovery_auth::principal::AgeRestrictionKind;
use crate::opds_catalog_access::OpdsBookAuthorEntry;

#[derive(Clone)]
pub(super) struct PersistedLibrary {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) last_modified: String,
}

#[derive(Clone)]
pub(super) struct PersistedSeries {
    pub(super) id: String,
    pub(super) library_id: String,
    pub(super) title: String,
    pub(super) summary: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
}

pub(super) struct PersistedSeriesBook {
    pub(super) id: String,
    pub(super) series_id: String,
    pub(super) title: String,
    pub(super) series_title: String,
    pub(super) number: String,
    pub(super) number_sort: f64,
    pub(super) summary: String,
    pub(super) isbn: Option<String>,
    pub(super) authors: Vec<OpdsBookAuthorEntry>,
    pub(super) tags: Vec<String>,
    pub(super) file_name: String,
    pub(super) file_size: i64,
    pub(super) media_type: String,
    pub(super) page_count: i64,
    pub(super) epub_divina_compatible: bool,
    pub(super) last_read: Option<i64>,
    pub(super) last_read_date: Option<String>,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
    pub(super) release_date: Option<String>,
}

pub(super) struct PersistedReadlist {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) last_modified: String,
    pub(super) ordered: bool,
}

pub(super) struct PersistedReadlistBook {
    pub(super) id: String,
    pub(super) series_id: String,
    pub(super) title: String,
    pub(super) series_title: String,
    pub(super) number: String,
    pub(super) number_sort: f64,
    pub(super) summary: String,
    pub(super) isbn: Option<String>,
    pub(super) authors: Vec<OpdsBookAuthorEntry>,
    pub(super) tags: Vec<String>,
    pub(super) file_name: String,
    pub(super) file_size: i64,
    pub(super) media_type: String,
    pub(super) media_status: Option<String>,
    pub(super) page_count: i64,
    pub(super) epub_divina_compatible: bool,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
    pub(super) release_date: Option<String>,
}

pub(super) struct PersistedSeriesSearchResult {
    pub(super) id: String,
    pub(super) title: String,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
}

pub(super) struct PersistedBookSearchResult {
    pub(super) id: String,
    pub(super) series_id: String,
    pub(super) title: String,
    pub(super) series_title: String,
    pub(super) number: String,
    pub(super) number_sort: f64,
    pub(super) summary: String,
    pub(super) isbn: Option<String>,
    pub(super) authors: Vec<OpdsBookAuthorEntry>,
    pub(super) tags: Vec<String>,
    pub(super) file_name: String,
    pub(super) file_size: i64,
    pub(super) media_type: String,
    pub(super) page_count: i64,
    pub(super) epub_divina_compatible: bool,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
    pub(super) release_date: Option<String>,
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
    pub(super) series_title: String,
    pub(super) number: String,
    pub(super) summary: String,
    pub(super) authors: Vec<String>,
    pub(super) file_name: String,
    pub(super) file_size: i64,
    pub(super) media_type: String,
    pub(super) page_count: i64,
    pub(super) epub_divina_compatible: bool,
    pub(super) last_read: Option<i64>,
    pub(super) last_read_date: Option<String>,
    pub(super) library_id: String,
    pub(super) age_rating: Option<u16>,
    pub(super) sharing_labels: Vec<String>,
    pub(super) last_modified: String,
}

pub(super) struct PersistedCollection {
    pub(super) id: String,
    pub(super) name: String,
    pub(super) last_modified: String,
    pub(super) ordered: bool,
}

#[derive(Clone, Debug, Default)]
pub(super) struct OpdsRestrictions {
    pub(super) age: Option<u16>,
    pub(super) age_restriction: Option<AgeRestrictionKind>,
    pub(super) labels_allow: Vec<String>,
    pub(super) labels_exclude: Vec<String>,
}
