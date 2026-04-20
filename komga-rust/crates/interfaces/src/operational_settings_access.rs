use komga_application::identity_access::AuthUser;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedServerSettings {
    pub delete_empty_collections: bool,
    pub delete_empty_read_lists: bool,
    pub remember_me_key: String,
    pub remember_me_duration_days: u64,
    pub thumbnail_size: &'static str,
    pub task_pool_size: u64,
    pub server_port: Option<u16>,
    pub server_context_path: Option<String>,
    pub kobo_proxy: bool,
    pub kobo_port: Option<u16>,
}

#[derive(Clone)]
pub struct TransientBookPage {
    pub number: u32,
    pub file_name: String,
    pub media_type: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub size_bytes: Option<u64>,
}

#[derive(Clone)]
pub struct TransientBookAnalysis {
    pub status: String,
    pub media_type: String,
    pub page_count: u32,
    pub pages: Vec<TransientBookPage>,
    pub files: Vec<String>,
    pub comment: String,
    pub number: Option<f64>,
    pub series_id: Option<String>,
}

#[derive(Clone)]
pub struct TransientBookFileMetadata {
    pub file_last_modified_unix_nanos: i128,
    pub size_bytes: u64,
}

#[derive(Clone)]
pub struct PageHashThumbnail {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashDeleteTargetPage {
    pub file_hash: String,
    pub file_size: i64,
    pub file_name: String,
    pub media_type: String,
    pub page_number: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageHashDeleteTarget {
    pub book_id: String,
    pub pages: Vec<PageHashDeleteTargetPage>,
}

pub enum ClaimInitialAdminUserResult {
    Created(Box<AuthUser>),
    AlreadyClaimed,
}
