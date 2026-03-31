#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityThumbnailRecord {
    pub id: String,
    pub book_id: String,
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityThumbnailBinary {
    pub media_type: String,
    pub thumbnail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesThumbnailRecord {
    pub id: String,
    pub series_id: String,
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlistThumbnailRecord {
    pub id: String,
    pub readlist_id: String,
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumbnail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionThumbnailRecord {
    pub id: String,
    pub collection_id: String,
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub file_size: i64,
    pub width: i64,
    pub height: i64,
    pub thumbnail: Vec<u8>,
}
