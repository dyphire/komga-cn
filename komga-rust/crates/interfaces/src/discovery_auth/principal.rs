use komga_application::identity_access::{AuthUser, user_is_admin, user_query_restrictions};
use komga_domain::discovery::{QueryRestrictions, content_allowed_by_restrictions};

use super::utils::intersection;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryPrincipal {
    pub(crate) user_id: String,
    pub(crate) is_admin: bool,
    pub(crate) shared_all_libraries: bool,
    pub(crate) shared_library_ids: Vec<String>,
    pub(crate) restrictions: QueryRestrictions,
}

impl DiscoveryPrincipal {
    pub(crate) fn is_admin(&self) -> bool {
        self.is_admin
    }

    pub(crate) fn can_access_all_libraries(&self) -> bool {
        self.shared_all_libraries || self.is_admin()
    }

    pub(crate) fn can_access_library(&self, library_id: &str) -> bool {
        self.can_access_all_libraries()
            || self
                .shared_library_ids
                .iter()
                .any(|candidate| candidate == library_id)
    }

    pub(crate) fn authorized_library_ids(
        &self,
        requested_library_ids: Option<&[String]>,
    ) -> Option<Vec<String>> {
        match (self.can_access_all_libraries(), requested_library_ids) {
            (false, Some(requested)) => Some(intersection(requested, &self.shared_library_ids)),
            (false, None) => Some(self.shared_library_ids.clone()),
            (true, Some(requested)) => Some(requested.to_vec()),
            (true, None) => None,
        }
    }

    pub(crate) fn is_content_allowed(
        &self,
        age_rating: Option<u32>,
        sharing_labels: &[String],
    ) -> bool {
        content_allowed_by_restrictions(&self.restrictions, age_rating, sharing_labels)
    }
}

pub(crate) fn principal_from_user(user: &AuthUser) -> Option<DiscoveryPrincipal> {
    let user_id = user.id.trim().to_string();
    if user_id.is_empty() {
        return None;
    }

    let shared_library_ids = user
        .shared_library_ids
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|library_id| !library_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();

    Some(DiscoveryPrincipal {
        user_id,
        is_admin: user_is_admin(user),
        shared_all_libraries: user.shared_all_libraries,
        shared_library_ids,
        restrictions: user_query_restrictions(user),
    })
}
