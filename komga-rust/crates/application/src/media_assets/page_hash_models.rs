#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashThumbnail {
    pub bytes: Vec<u8>,
    pub media_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashDeleteTargetPage {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageHashDeleteTarget {
    pub book_id: String,
    pub pages: Vec<PageHashDeleteTargetPage>,
}
