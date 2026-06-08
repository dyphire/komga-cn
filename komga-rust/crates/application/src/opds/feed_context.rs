use std::collections::HashSet;

use komga_domain::discovery::{
    AgeRestrictionKind as DomainAgeRestrictionKind, QueryRestrictions,
    content_allowed_by_restrictions,
};

use crate::identity_access::{
    AuthUser, user_id, user_shared_all_libraries, user_shared_library_ids,
};

use super::records::{OpdsBookFeedEntry, OpdsSeriesEntry};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OpdsAgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpdsFeedUserContext {
    pub user_id: String,
    pub allowed_library_ids: Option<HashSet<String>>,
    pub age: Option<u16>,
    pub age_restriction: Option<OpdsAgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
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
        let age = user
            .age_restriction
            .as_ref()
            .and_then(|restriction| u16::try_from(restriction.age).ok());
        let age_restriction =
            user.age_restriction.as_ref().and_then(|restriction| {
                match restriction.restriction.trim().to_ascii_uppercase().as_str() {
                    "ALLOW_ONLY" => Some(OpdsAgeRestrictionKind::AllowOnly),
                    "EXCLUDE" => Some(OpdsAgeRestrictionKind::Exclude),
                    _ => None,
                }
            });

        Self {
            user_id: user_id(user).to_string(),
            allowed_library_ids,
            age,
            age_restriction,
            labels_allow: normalized_labels(&user.labels_allow),
            labels_exclude: normalized_labels(&user.labels_exclude),
        }
    }

    pub fn can_access_library(&self, library_id: &str) -> bool {
        match &self.allowed_library_ids {
            None => true,
            Some(ids) => ids.contains(library_id),
        }
    }

    pub fn content_allowed(&self, age_rating: Option<u16>, sharing_labels: &[String]) -> bool {
        let restrictions = QueryRestrictions {
            age: self.age,
            age_restriction: self.age_restriction.map(|kind| match kind {
                OpdsAgeRestrictionKind::AllowOnly => DomainAgeRestrictionKind::AllowOnly,
                OpdsAgeRestrictionKind::Exclude => DomainAgeRestrictionKind::Exclude,
            }),
            labels_allow: self.labels_allow.clone(),
            labels_exclude: self.labels_exclude.clone(),
        };
        content_allowed_by_restrictions(&restrictions, age_rating, sharing_labels)
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

fn normalized_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|label| label.trim().to_ascii_lowercase())
        .filter(|label| !label.is_empty())
        .collect()
}
