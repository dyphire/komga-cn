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

impl QueryRestrictions {
    pub fn is_restricted(&self) -> bool {
        self.age.is_some()
            || self.age_restriction.is_some()
            || !self.labels_allow.is_empty()
            || !self.labels_exclude.is_empty()
    }
}

pub fn content_allowed_by_restrictions(
    restrictions: &QueryRestrictions,
    age_rating: Option<u32>,
    sharing_labels: &[String],
) -> bool {
    let labels = normalized_sharing_labels(sharing_labels);

    let age_allowed = if restrictions.age_restriction == Some(AgeRestrictionKind::AllowOnly) {
        restrictions
            .age
            .map(|age_limit| age_rating.is_some_and(|age| age <= u32::from(age_limit)))
    } else {
        None
    };
    let label_allowed = if restrictions.labels_allow.is_empty() {
        None
    } else {
        Some(
            restrictions
                .labels_allow
                .iter()
                .any(|candidate| labels.contains(candidate)),
        )
    };

    let allowed = match (age_allowed, label_allowed) {
        (None, label_allowed) => label_allowed != Some(false),
        (age_allowed, None) => age_allowed != Some(false),
        (age_allowed, label_allowed) => age_allowed != Some(false) || label_allowed != Some(false),
    };
    if !allowed {
        return false;
    }

    let age_denied = if restrictions.age_restriction == Some(AgeRestrictionKind::Exclude) {
        restrictions
            .age
            .is_some_and(|age_limit| age_rating.is_some_and(|age| age >= u32::from(age_limit)))
    } else {
        false
    };
    let label_denied = if restrictions.labels_exclude.is_empty() {
        false
    } else {
        restrictions
            .labels_exclude
            .iter()
            .any(|candidate| labels.contains(candidate))
    };

    !age_denied && !label_denied
}

fn normalized_sharing_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .filter(|value| !value.is_empty())
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryQueryContext {
    pub user_id: Option<UserId>,
    pub is_admin: bool,
    pub authorized_library_ids: Option<Vec<LibraryId>>,
    pub restrictions: Option<QueryRestrictions>,
}
