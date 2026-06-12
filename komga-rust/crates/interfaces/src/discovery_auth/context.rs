use komga_domain::discovery::QueryRestrictions;

use super::principal::DiscoveryPrincipal;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DiscoveryQueryContext {
    pub(crate) user_id: Option<String>,
    pub(crate) is_admin: bool,
    pub(crate) authorized_library_ids: Option<Vec<String>>,
    pub(crate) restrictions: Option<QueryRestrictions>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DetailContentContext {
    pub(crate) age_rating: Option<u32>,
    pub(crate) sharing_labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DetailResourceContext {
    pub(crate) library_id: Option<String>,
    pub(crate) content: Option<DetailContentContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DetailAccessDenial {
    Unauthorized,
    Forbidden,
    NotFound,
    StorageFailure,
}

pub(crate) fn to_query_context(
    principal: &DiscoveryPrincipal,
    requested_library_ids: Option<&[String]>,
) -> DiscoveryQueryContext {
    DiscoveryQueryContext {
        user_id: Some(principal.user_id.clone()),
        is_admin: principal.is_admin(),
        authorized_library_ids: principal.authorized_library_ids(requested_library_ids),
        restrictions: restrictions_for_query(&principal.restrictions),
    }
}

fn restrictions_for_query(restrictions: &QueryRestrictions) -> Option<QueryRestrictions> {
    let has_restrictions = restrictions.age.is_some()
        || restrictions.age_restriction.is_some()
        || !restrictions.labels_allow.is_empty()
        || !restrictions.labels_exclude.is_empty();

    has_restrictions.then(|| restrictions.clone())
}
