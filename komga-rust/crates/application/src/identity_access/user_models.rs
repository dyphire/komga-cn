use serde::{Deserialize, Serialize};

use komga_domain::discovery::{AgeRestrictionKind as DomainAgeRestrictionKind, QueryRestrictions};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedApiKey {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub comment: String,
    pub created_date: Option<String>,
    pub last_modified_date: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedAuthenticationActivity {
    pub user_id: Option<String>,
    pub email: Option<String>,
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub date_time: String,
    pub source: Option<String>,
    pub api_key_id: Option<String>,
    pub api_key_comment: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedApiKeyMetadata {
    pub id: String,
    pub comment: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthUser {
    pub id: String,
    pub email: String,
    pub password: String,
    pub roles: Vec<AuthUserRole>,
    pub shared_all_libraries: bool,
    pub shared_library_ids: Vec<String>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
    pub age_restriction: Option<AuthUserAgeRestriction>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthUserSessionSnapshot {
    pub id: String,
    pub email: String,
    pub roles: Vec<String>,
    pub shared_all_libraries: bool,
    pub shared_library_ids: Vec<String>,
    pub labels_allow: Vec<String>,
    pub labels_exclude: Vec<String>,
    pub age_restriction: Option<AuthUserAgeRestrictionSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthUserAgeRestrictionSnapshot {
    pub age: i64,
    pub restriction: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AuthUserAgeRestriction {
    pub age: i64,
    pub restriction: AuthUserAgeRestrictionKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthUserAgeRestrictionKind {
    AllowOnly,
    Exclude,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthOutcome {
    Valid(Box<AuthUser>),
    Invalid,
    Missing,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum AuthUserRole {
    Admin,
    FileDownload,
    PageStreaming,
    KoboSync,
    KoreaderSync,
}

impl AuthUserRole {
    pub const CLAIM_ROLES: [Self; 5] = [
        Self::Admin,
        Self::FileDownload,
        Self::PageStreaming,
        Self::KoboSync,
        Self::KoreaderSync,
    ];
    pub const VIRTUAL_USER_ROLE_NAME: &'static str = "USER";

    pub fn claim_roles() -> impl Iterator<Item = Self> {
        Self::CLAIM_ROLES.into_iter()
    }

    pub fn claim_role_names() -> impl Iterator<Item = &'static str> {
        Self::claim_roles().map(Self::persisted_name)
    }

    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::Admin => "ADMIN",
            Self::FileDownload => "FILE_DOWNLOAD",
            Self::PageStreaming => "PAGE_STREAMING",
            Self::KoboSync => "KOBO_SYNC",
            Self::KoreaderSync => "KOREADER_SYNC",
        }
    }

    pub fn from_persisted_name(value: &str) -> Option<Self> {
        match value {
            "ADMIN" => Some(Self::Admin),
            "FILE_DOWNLOAD" => Some(Self::FileDownload),
            "PAGE_STREAMING" => Some(Self::PageStreaming),
            "KOBO_SYNC" => Some(Self::KoboSync),
            "KOREADER_SYNC" => Some(Self::KoreaderSync),
            _ => None,
        }
    }
}

impl AuthUserAgeRestrictionKind {
    pub fn persisted_name(self) -> &'static str {
        match self {
            Self::AllowOnly => "ALLOW_ONLY",
            Self::Exclude => "EXCLUDE",
        }
    }

    pub fn from_persisted_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "ALLOW_ONLY" => Some(Self::AllowOnly),
            "EXCLUDE" => Some(Self::Exclude),
            _ => None,
        }
    }

    pub fn from_allow_only(value: bool) -> Self {
        if value {
            Self::AllowOnly
        } else {
            Self::Exclude
        }
    }
}

pub fn user_age_restriction_from_persisted_columns(
    age: Option<i64>,
    allow_only: Option<bool>,
) -> Option<AuthUserAgeRestriction> {
    match (age, allow_only) {
        (Some(age), Some(allow_only)) => Some(AuthUserAgeRestriction {
            age,
            restriction: AuthUserAgeRestrictionKind::from_allow_only(allow_only),
        }),
        _ => None,
    }
}

impl From<AuthUserAgeRestrictionKind> for DomainAgeRestrictionKind {
    fn from(value: AuthUserAgeRestrictionKind) -> Self {
        match value {
            AuthUserAgeRestrictionKind::AllowOnly => Self::AllowOnly,
            AuthUserAgeRestrictionKind::Exclude => Self::Exclude,
        }
    }
}

impl PersistedApiKey {
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn user_id(&self) -> &str {
        self.user_id.as_str()
    }

    pub fn key(&self) -> &str {
        self.key.as_str()
    }

    pub fn comment(&self) -> &str {
        self.comment.as_str()
    }

    pub fn created_date(&self) -> Option<&str> {
        self.created_date.as_deref()
    }

    pub fn last_modified_date(&self) -> Option<&str> {
        self.last_modified_date.as_deref()
    }
}

impl PersistedApiKeyMetadata {
    pub fn id(&self) -> &str {
        self.id.as_str()
    }

    pub fn comment(&self) -> &str {
        self.comment.as_str()
    }
}

impl PersistedAuthenticationActivity {
    pub fn user_id(&self) -> Option<&str> {
        self.user_id.as_deref()
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn ip(&self) -> Option<&str> {
        self.ip.as_deref()
    }

    pub fn user_agent(&self) -> Option<&str> {
        self.user_agent.as_deref()
    }

    pub fn success(&self) -> bool {
        self.success
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn date_time(&self) -> &str {
        self.date_time.as_str()
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn api_key_id(&self) -> Option<&str> {
        self.api_key_id.as_deref()
    }

    pub fn api_key_comment(&self) -> Option<&str> {
        self.api_key_comment.as_deref()
    }
}

pub fn user_id(user: &AuthUser) -> &str {
    user.id.as_str()
}

pub fn user_is_admin(user: &AuthUser) -> bool {
    user_has_role(user, AuthUserRole::Admin)
}

pub fn user_has_role(user: &AuthUser, role: AuthUserRole) -> bool {
    user.roles.contains(&role)
}

pub fn user_persisted_role_names(user: &AuthUser) -> impl Iterator<Item = &'static str> + '_ {
    user.roles.iter().copied().map(AuthUserRole::persisted_name)
}

pub fn user_response_role_names(user: &AuthUser) -> Vec<&'static str> {
    let mut roles = user_persisted_role_names(user).collect::<Vec<_>>();
    roles.push(AuthUserRole::VIRTUAL_USER_ROLE_NAME);
    roles.sort_unstable();
    roles.dedup();
    roles
}

pub fn user_roles_from_persisted_names<I, S>(roles: I) -> Vec<AuthUserRole>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut roles = roles
        .into_iter()
        .filter(|role| role.as_ref() != AuthUserRole::VIRTUAL_USER_ROLE_NAME)
        .filter_map(|role| AuthUserRole::from_persisted_name(role.as_ref()))
        .collect::<Vec<_>>();
    roles.sort_unstable();
    roles.dedup();
    roles
}

pub fn user_shared_all_libraries(user: &AuthUser) -> bool {
    user.shared_all_libraries
}

pub fn user_shared_library_ids(user: &AuthUser) -> &[String] {
    user.shared_library_ids.as_slice()
}

pub fn user_query_restrictions(user: &AuthUser) -> QueryRestrictions {
    QueryRestrictions {
        age: user
            .age_restriction
            .as_ref()
            .and_then(|restriction| u16::try_from(restriction.age).ok()),
        age_restriction: user
            .age_restriction
            .as_ref()
            .map(|restriction| restriction.restriction.into()),
        labels_allow: normalized_user_labels(&user.labels_allow),
        labels_exclude: normalized_user_labels(&user.labels_exclude),
    }
}

pub fn user_session_snapshot(user: &AuthUser) -> AuthUserSessionSnapshot {
    AuthUserSessionSnapshot {
        id: user.id.clone(),
        email: user.email.clone(),
        roles: user_persisted_role_names(user)
            .map(str::to_string)
            .collect(),
        shared_all_libraries: user.shared_all_libraries,
        shared_library_ids: user.shared_library_ids.clone(),
        labels_allow: user.labels_allow.clone(),
        labels_exclude: user.labels_exclude.clone(),
        age_restriction: user.age_restriction.as_ref().map(|age_restriction| {
            AuthUserAgeRestrictionSnapshot {
                age: age_restriction.age,
                restriction: age_restriction.restriction.persisted_name().to_string(),
            }
        }),
    }
}

pub fn user_from_session_snapshot(snapshot: &AuthUserSessionSnapshot) -> AuthUser {
    let age_restriction = snapshot.age_restriction.as_ref().map(|age_restriction| {
        let restriction =
            AuthUserAgeRestrictionKind::from_persisted_name(&age_restriction.restriction)
                .expect("session snapshot age restriction kind should use a known value");
        AuthUserAgeRestriction {
            age: age_restriction.age,
            restriction,
        }
    });

    AuthUser {
        id: snapshot.id.clone(),
        email: snapshot.email.clone(),
        password: String::new(),
        roles: user_roles_from_persisted_names(&snapshot.roles),
        shared_all_libraries: snapshot.shared_all_libraries,
        shared_library_ids: snapshot.shared_library_ids.clone(),
        labels_allow: snapshot.labels_allow.clone(),
        labels_exclude: snapshot.labels_exclude.clone(),
        age_restriction,
    }
}

fn normalized_user_labels(labels: &[String]) -> Vec<String> {
    labels
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn age_restriction_kind_survives_session_snapshot_as_persisted_name() {
        let user = AuthUser {
            id: "user-1".to_string(),
            email: "user@example.org".to_string(),
            password: String::new(),
            roles: vec![AuthUserRole::PageStreaming],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: Some(AuthUserAgeRestriction {
                age: 16,
                restriction: AuthUserAgeRestrictionKind::Exclude,
            }),
        };

        let snapshot = user_session_snapshot(&user);

        assert_eq!(
            snapshot.age_restriction,
            Some(AuthUserAgeRestrictionSnapshot {
                age: 16,
                restriction: "EXCLUDE".to_string(),
            })
        );
        assert_eq!(user_from_session_snapshot(&snapshot), user);
    }

    #[test]
    fn user_response_role_names_include_virtual_user_role() {
        let user = AuthUser {
            id: "user-1".to_string(),
            email: "user@example.org".to_string(),
            password: String::new(),
            roles: vec![AuthUserRole::PageStreaming],
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        };

        assert_eq!(
            user_response_role_names(&user),
            vec!["PAGE_STREAMING", "USER"]
        );
    }

    #[test]
    fn persisted_user_roles_ignore_virtual_user_role() {
        let roles = user_roles_from_persisted_names(["USER", "PAGE_STREAMING"]);

        assert_eq!(roles, vec![AuthUserRole::PageStreaming]);
    }

    #[test]
    fn persisted_user_age_restriction_restores_complete_columns() {
        assert_eq!(
            user_age_restriction_from_persisted_columns(None, None),
            None
        );
        assert_eq!(
            user_age_restriction_from_persisted_columns(Some(16), Some(false)),
            Some(AuthUserAgeRestriction {
                age: 16,
                restriction: AuthUserAgeRestrictionKind::Exclude,
            })
        );
    }
}
