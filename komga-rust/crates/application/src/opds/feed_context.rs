use std::collections::HashSet;

use komga_domain::discovery::{QueryRestrictions, content_allowed_by_restrictions};

use crate::identity_access::{
    AuthUser, user_id, user_query_restrictions, user_shared_all_libraries, user_shared_library_ids,
};

use super::records::{OpdsBookFeedEntry, OpdsSeriesEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpdsFeedUserContext {
    pub user_id: String,
    pub allowed_library_ids: Option<HashSet<String>>,
    pub restrictions: QueryRestrictions,
}

impl OpdsFeedUserContext {
    pub fn from_auth_user(user: &AuthUser) -> Self {
        let allowed_library_ids = if user_shared_all_libraries(user) {
            None
        } else {
            Some(
                user_shared_library_ids(user)
                    .iter()
                    .cloned()
                    .collect::<HashSet<_>>(),
            )
        };

        Self {
            user_id: user_id(user).to_string(),
            allowed_library_ids,
            restrictions: user_query_restrictions(user),
        }
    }

    pub fn can_access_library(&self, library_id: &str) -> bool {
        match &self.allowed_library_ids {
            None => true,
            Some(ids) => ids.contains(library_id),
        }
    }

    pub fn content_allowed(&self, age_rating: Option<u32>, sharing_labels: &[String]) -> bool {
        content_allowed_by_restrictions(&self.restrictions, age_rating, sharing_labels)
    }

    pub fn allowed_library_ids(&self) -> Option<&HashSet<String>> {
        self.allowed_library_ids.as_ref()
    }

    pub fn can_access_book_feed_entry(&self, book: &OpdsBookFeedEntry) -> bool {
        self.can_access_library(&book.library_id)
            && self.content_allowed(book.age_rating, &book.sharing_labels)
    }

    pub fn can_access_series_feed_entry(
        &self,
        series: &OpdsSeriesEntry,
        include_one_shots: bool,
    ) -> bool {
        (include_one_shots || !series.one_shot)
            && self.can_access_library(&series.library_id)
            && self.content_allowed(series.age_rating, &series.sharing_labels)
    }
}

pub struct OpdsPagedBooks {
    pub books: Vec<OpdsBookFeedEntry>,
    pub total_visible_books: usize,
    pub has_next: bool,
}

pub struct OpdsPagedSeries {
    pub series: Vec<OpdsSeriesEntry>,
    pub total_visible_series: usize,
    pub has_next: bool,
}
