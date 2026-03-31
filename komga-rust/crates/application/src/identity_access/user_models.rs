use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistedApiKey {
    pub id: String,
    pub user_id: String,
    pub key: String,
    pub comment: String,
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
    pub roles: Vec<String>,
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
    pub restriction: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AuthOutcome {
    Valid(Box<AuthUser>),
    Invalid,
    Missing,
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
    user.roles.iter().any(|role| role == "ADMIN")
}

pub fn user_has_role(user: &AuthUser, role: &str) -> bool {
    user.roles.iter().any(|candidate| candidate == role)
}

pub fn user_shared_all_libraries(user: &AuthUser) -> bool {
    user.shared_all_libraries
}

pub fn user_shared_library_ids(user: &AuthUser) -> &[String] {
    user.shared_library_ids.as_slice()
}

pub fn user_payload_json(user: &AuthUser) -> Value {
    let mut roles = BTreeSet::new();
    for role in &user.roles {
        let role = role.trim();
        if role.is_empty() {
            continue;
        }
        roles.insert(role.to_string());
    }
    roles.insert("USER".to_string());

    json!({
        "id": user.id,
        "email": user.email,
        "roles": roles,
        "sharedAllLibraries": user.shared_all_libraries,
        "sharedLibrariesIds": user.shared_library_ids,
        "labelsAllow": user.labels_allow,
        "labelsExclude": user.labels_exclude,
        "ageRestriction": user.age_restriction.as_ref().map(|age_restriction| {
            json!({
                "age": age_restriction.age,
                "restriction": age_restriction.restriction,
            })
        }),
    })
}

pub fn user_session_snapshot(user: &AuthUser) -> AuthUserSessionSnapshot {
    AuthUserSessionSnapshot {
        id: user.id.clone(),
        email: user.email.clone(),
        roles: user.roles.clone(),
        shared_all_libraries: user.shared_all_libraries,
        shared_library_ids: user.shared_library_ids.clone(),
        labels_allow: user.labels_allow.clone(),
        labels_exclude: user.labels_exclude.clone(),
        age_restriction: user.age_restriction.as_ref().map(|age_restriction| {
            AuthUserAgeRestrictionSnapshot {
                age: age_restriction.age,
                restriction: age_restriction.restriction.clone(),
            }
        }),
    }
}

pub fn user_from_session_snapshot(snapshot: &AuthUserSessionSnapshot) -> AuthUser {
    AuthUser {
        id: snapshot.id.clone(),
        email: snapshot.email.clone(),
        password: String::new(),
        roles: snapshot.roles.clone(),
        shared_all_libraries: snapshot.shared_all_libraries,
        shared_library_ids: snapshot.shared_library_ids.clone(),
        labels_allow: snapshot.labels_allow.clone(),
        labels_exclude: snapshot.labels_exclude.clone(),
        age_restriction: snapshot.age_restriction.as_ref().map(|age_restriction| {
            AuthUserAgeRestriction {
                age: age_restriction.age,
                restriction: age_restriction.restriction.clone(),
            }
        }),
    }
}
