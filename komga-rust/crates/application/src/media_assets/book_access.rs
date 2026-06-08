use komga_domain::discovery::{
    AgeRestrictionKind, QueryRestrictions, content_allowed_by_restrictions,
};

use crate::identity_access::{AuthUser, user_shared_all_libraries, user_shared_library_ids};

pub(super) struct BookAccessContext {
    allowed_library_ids: Option<Vec<String>>,
    age: Option<u16>,
    age_restriction: Option<AgeRestrictionKind>,
    labels_allow: Vec<String>,
    labels_exclude: Vec<String>,
}

impl BookAccessContext {
    pub(super) fn from_auth_user(user: &AuthUser) -> Self {
        let allowed_library_ids = if user_shared_all_libraries(user) {
            None
        } else {
            Some(user_shared_library_ids(user).to_vec())
        };
        let age = user
            .age_restriction
            .as_ref()
            .and_then(|restriction| u16::try_from(restriction.age).ok());
        let age_restriction =
            user.age_restriction.as_ref().and_then(|restriction| {
                match restriction.restriction.trim().to_ascii_uppercase().as_str() {
                    "ALLOW_ONLY" => Some(AgeRestrictionKind::AllowOnly),
                    "EXCLUDE" => Some(AgeRestrictionKind::Exclude),
                    _ => None,
                }
            });

        Self {
            allowed_library_ids,
            age,
            age_restriction,
            labels_allow: normalized_labels(&user.labels_allow),
            labels_exclude: normalized_labels(&user.labels_exclude),
        }
    }

    pub(super) fn can_access_library(&self, library_id: &str) -> bool {
        match &self.allowed_library_ids {
            None => true,
            Some(ids) => ids.iter().any(|id| id == library_id),
        }
    }

    pub(super) fn content_allowed(
        &self,
        age_rating: Option<u16>,
        sharing_labels: &[String],
    ) -> bool {
        let restrictions = QueryRestrictions {
            age: self.age,
            age_restriction: self.age_restriction,
            labels_allow: self.labels_allow.clone(),
            labels_exclude: self.labels_exclude.clone(),
        };
        content_allowed_by_restrictions(&restrictions, age_rating, sharing_labels)
    }
}

fn normalized_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}
