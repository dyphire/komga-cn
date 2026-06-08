use komga_domain::discovery::{
    AgeRestrictionKind, QueryRestrictions, content_allowed_by_restrictions,
};

use super::super::user_models::{AuthUser, user_is_admin};

pub struct KoboSyncAccessPolicy<'a> {
    user: &'a AuthUser,
}

impl<'a> KoboSyncAccessPolicy<'a> {
    pub fn new(user: &'a AuthUser) -> Self {
        Self { user }
    }

    pub fn can_access_book(
        &self,
        library_id: &str,
        age_rating: Option<u16>,
        sharing_labels: &[String],
    ) -> bool {
        self.can_access_library(library_id)
            && content_allowed_by_restrictions(
                &query_restrictions(self.user),
                age_rating,
                sharing_labels,
            )
    }

    fn can_access_library(&self, library_id: &str) -> bool {
        self.user.shared_all_libraries
            || user_is_admin(self.user)
            || self
                .user
                .shared_library_ids
                .iter()
                .any(|shared_library_id| shared_library_id == library_id)
    }
}

fn query_restrictions(user: &AuthUser) -> QueryRestrictions {
    QueryRestrictions {
        age: user
            .age_restriction
            .as_ref()
            .and_then(|restriction| u16::try_from(restriction.age).ok()),
        age_restriction: user.age_restriction.as_ref().and_then(|restriction| {
            match restriction.restriction.as_str() {
                value if value.eq_ignore_ascii_case("ALLOW_ONLY") => {
                    Some(AgeRestrictionKind::AllowOnly)
                }
                value if value.eq_ignore_ascii_case("EXCLUDE") => Some(AgeRestrictionKind::Exclude),
                _ => None,
            }
        }),
        labels_allow: normalized_user_labels(&user.labels_allow),
        labels_exclude: normalized_user_labels(&user.labels_exclude),
    }
}

fn normalized_user_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}
