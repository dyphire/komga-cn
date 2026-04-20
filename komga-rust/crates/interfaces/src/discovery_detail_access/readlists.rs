#[derive(Clone)]
pub struct PersistedReadlistRecord {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub ordered: bool,
    pub created_date: String,
    pub last_modified_date: String,
}

#[derive(Clone)]
pub struct PersistedReadlistBookRecord {
    pub book_id: String,
    pub library_id: String,
}

#[derive(Clone)]
pub struct PersistedComicrackMatchCandidateRecord {
    pub series_id: String,
    pub series_title: String,
    pub series_release_date: Option<String>,
    pub book_id: String,
    pub book_title: String,
    pub book_number: String,
}

#[derive(Clone)]
pub struct PersistedBookAuthorRecord {
    pub name: String,
    pub role: String,
}
