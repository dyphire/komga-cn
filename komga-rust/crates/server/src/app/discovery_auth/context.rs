use super::principal::{AgeRestrictionKind, ContentRestrictions, DiscoveryPrincipal};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryRestrictions {
    pub age: Option<u16>,
    pub age_restriction: Option<AgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryQueryContext {
    pub user_id: Option<String>,
    pub is_admin: bool,
    pub authorized_library_ids: Option<Vec<String>>,
    pub restrictions: Option<QueryRestrictions>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailContentContext {
    pub age_rating: Option<u16>,
    pub sharing_labels: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DetailResourceContext {
    pub library_id: Option<String>,
    pub content: Option<DetailContentContext>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DetailAccessDenial {
    Unauthorized,
    Forbidden,
    NotFound,
}

pub fn to_query_context(
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

fn restrictions_for_query(restrictions: &ContentRestrictions) -> Option<QueryRestrictions> {
    let has_restrictions = restrictions.age.is_some()
        || restrictions.age_restriction.is_some()
        || !restrictions.labels_allow.is_empty()
        || !restrictions.labels_exclude.is_empty();

    has_restrictions.then(|| QueryRestrictions {
        age: restrictions.age,
        age_restriction: restrictions.age_restriction,
        labels_allow: restrictions.labels_allow.clone(),
        labels_exclude: restrictions.labels_exclude.clone(),
    })
}
