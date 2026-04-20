use serde_json::Value;

use super::utils::{intersection, normalized_labels, normalized_sharing_labels};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ContentRestrictions {
    pub age: Option<u16>,
    pub age_restriction: Option<AgeRestrictionKind>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
}

impl ContentRestrictions {
    pub fn is_restricted(&self) -> bool {
        self.age.is_some()
            || self.age_restriction.is_some()
            || !self.labels_allow.is_empty()
            || !self.labels_exclude.is_empty()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveryPrincipal {
    pub user_id: String,
    pub roles: Vec<String>,
    pub shared_all_libraries: bool,
    pub shared_library_ids: Vec<String>,
    pub restrictions: ContentRestrictions,
}

impl DiscoveryPrincipal {
    pub fn is_admin(&self) -> bool {
        self.roles.iter().any(|role| role == "ADMIN")
    }

    pub fn can_access_all_libraries(&self) -> bool {
        self.shared_all_libraries || self.is_admin()
    }

    pub fn can_access_library(&self, library_id: &str) -> bool {
        self.can_access_all_libraries()
            || self
                .shared_library_ids
                .iter()
                .any(|candidate| candidate == library_id)
    }

    pub fn authorized_library_ids(
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

    pub fn is_content_allowed(&self, age_rating: Option<u32>, sharing_labels: &[String]) -> bool {
        let labels = normalized_sharing_labels(sharing_labels);

        let age_allowed =
            if self.restrictions.age_restriction == Some(AgeRestrictionKind::AllowOnly) {
                self.restrictions
                    .age
                    .map(|age_limit| age_rating.is_some_and(|age| age <= u32::from(age_limit)))
            } else {
                None
            };

        let label_allowed = if self.restrictions.labels_allow.is_empty() {
            None
        } else {
            Some(
                self.restrictions
                    .labels_allow
                    .iter()
                    .any(|candidate| labels.contains(candidate)),
            )
        };

        let allowed = match (age_allowed, label_allowed) {
            (None, label_allowed) => label_allowed != Some(false),
            (age_allowed, None) => age_allowed != Some(false),
            (age_allowed, label_allowed) => {
                age_allowed != Some(false) || label_allowed != Some(false)
            }
        };
        if !allowed {
            return false;
        }

        let age_denied = if self.restrictions.age_restriction == Some(AgeRestrictionKind::Exclude) {
            self.restrictions
                .age
                .is_some_and(|age_limit| age_rating.is_some_and(|age| age >= u32::from(age_limit)))
        } else {
            false
        };

        let label_denied = if self.restrictions.labels_exclude.is_empty() {
            false
        } else {
            self.restrictions
                .labels_exclude
                .iter()
                .any(|candidate| labels.contains(candidate))
        };

        !age_denied && !label_denied
    }
}

pub fn principal_from_user_payload(payload: &Value) -> Option<DiscoveryPrincipal> {
    let user_id = payload.get("id")?.as_str()?.trim().to_string();
    if user_id.is_empty() {
        return None;
    }

    let roles = payload
        .get("roles")
        .and_then(Value::as_array)
        .map(|roles| {
            roles
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|role| !role.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let shared_all_libraries = payload
        .get("sharedAllLibraries")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let shared_library_ids = payload
        .get("sharedLibrariesIds")
        .and_then(Value::as_array)
        .map(|library_ids| {
            library_ids
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|library_id| !library_id.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let age = payload
        .get("ageRestriction")
        .and_then(|value| value.get("age"))
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok());
    let age_restriction = payload
        .get("ageRestriction")
        .and_then(|value| value.get("restriction"))
        .and_then(Value::as_str)
        .and_then(|value| match value.trim().to_ascii_uppercase().as_str() {
            "ALLOW_ONLY" => Some(AgeRestrictionKind::AllowOnly),
            "EXCLUDE" => Some(AgeRestrictionKind::Exclude),
            _ => None,
        });

    let labels_allow = payload
        .get("labelsAllow")
        .and_then(Value::as_array)
        .map(|labels| normalized_labels(labels))
        .unwrap_or_default();
    let labels_exclude = payload
        .get("labelsExclude")
        .and_then(Value::as_array)
        .map(|labels| normalized_labels(labels))
        .unwrap_or_default();

    Some(DiscoveryPrincipal {
        user_id,
        roles,
        shared_all_libraries,
        shared_library_ids,
        restrictions: ContentRestrictions {
            age,
            age_restriction,
            labels_allow,
            labels_exclude,
        },
    })
}
