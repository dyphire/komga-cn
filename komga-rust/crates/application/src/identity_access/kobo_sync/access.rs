use komga_domain::discovery::content_allowed_by_restrictions;

use super::super::user_models::{AuthUser, user_is_admin, user_query_restrictions};

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
        age_rating: Option<u32>,
        sharing_labels: &[String],
    ) -> bool {
        self.can_access_library(library_id)
            && content_allowed_by_restrictions(
                &user_query_restrictions(self.user),
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
