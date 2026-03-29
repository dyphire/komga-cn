use crate::common_ids::{LibraryId, UserId};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRestrictions {
    pub age: Option<u16>,
    pub age_restriction: Option<AgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryQueryContext {
    pub user_id: Option<UserId>,
    pub is_admin: bool,
    pub authorized_library_ids: Option<Vec<LibraryId>>,
    pub restrictions: Option<QueryRestrictions>,
}

impl DiscoveryQueryContext {
    pub fn allow_all() -> Self {
        Self {
            user_id: None,
            is_admin: true,
            authorized_library_ids: None,
            restrictions: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DirectBrowseBooksListFamily {
    BrowseSeriesPaged,
    BrowseBookSiblingsUnpaged,
    BrowseOneshotBootstrap,
}
