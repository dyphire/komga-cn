use komga_domain::discovery::{QueryRestrictions, content_allowed_by_restrictions};

use crate::identity_access::{
    AuthUser, user_query_restrictions, user_shared_all_libraries, user_shared_library_ids,
};

pub(super) struct BookAccessContext {
    allowed_library_ids: Option<Vec<String>>,
    restrictions: QueryRestrictions,
}

impl BookAccessContext {
    pub(super) fn from_auth_user(user: &AuthUser) -> Self {
        let allowed_library_ids = if user_shared_all_libraries(user) {
            None
        } else {
            Some(user_shared_library_ids(user).to_vec())
        };

        Self {
            allowed_library_ids,
            restrictions: user_query_restrictions(user),
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
        age_rating: Option<u32>,
        sharing_labels: &[String],
    ) -> bool {
        content_allowed_by_restrictions(&self.restrictions, age_rating, sharing_labels)
    }
}
