#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityThumbnailRecord {
    pub id: String,
    pub thumbnail_type: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntityThumbnailBinary {
    pub media_type: String,
    pub thumbnail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SeriesThumbnailRecord {
    pub id: String,
    pub thumbnail_type: String,
    pub selected: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReadlistThumbnailRecord {
    pub id: String,
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub thumbnail: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionThumbnailRecord {
    pub id: String,
    pub thumbnail_type: String,
    pub selected: bool,
    pub media_type: String,
    pub thumbnail: Vec<u8>,
}
