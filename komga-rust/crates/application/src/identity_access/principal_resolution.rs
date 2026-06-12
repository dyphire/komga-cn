use super::user_models::{AuthOutcome, AuthUser, AuthUserRole, user_has_role, user_id};

pub fn resolve_kobo_user(
    api_key_user: Option<AuthOutcome>,
    session_user: Option<AuthUser>,
) -> Option<AuthUser> {
    if let Some(AuthOutcome::Valid(user)) = api_key_user {
        Some(*user)
    } else {
        session_user
    }
}

pub fn resolve_koreader_user_id(session_user: Option<&AuthUser>) -> Option<String> {
    role_scoped_session_user(session_user, AuthUserRole::KoreaderSync)
        .map(|user| user_id(user).to_string())
}

pub fn koreader_authorized(session_user: Option<&AuthUser>) -> bool {
    role_scoped_session_user(session_user, AuthUserRole::KoreaderSync).is_some()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfiguredApiKeyIdentity {
    pub id: String,
    pub comment: String,
}

pub fn configured_api_key_identity(
    presented_token: &str,
    configured_api_key: Option<&str>,
    configured_api_key_id: Option<&str>,
    configured_api_key_comment: Option<&str>,
) -> ConfiguredApiKeyIdentity {
    if configured_api_key.is_some_and(|value| value == presented_token) {
        ConfiguredApiKeyIdentity {
            id: configured_api_key_id.unwrap_or("unknown").to_string(),
            comment: configured_api_key_comment.unwrap_or("unknown").to_string(),
        }
    } else {
        ConfiguredApiKeyIdentity {
            id: "unknown".to_string(),
            comment: "unknown".to_string(),
        }
    }
}

fn role_scoped_session_user(
    session_user: Option<&AuthUser>,
    required_role: AuthUserRole,
) -> Option<&AuthUser> {
    session_user.filter(|user| user_has_role(user, required_role))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_user_with_roles(roles: &[AuthUserRole]) -> AuthUser {
        AuthUser {
            id: "user-1".to_string(),
            email: "user@example.org".to_string(),
            password: "password".to_string(),
            roles: roles.to_vec(),
            shared_all_libraries: true,
            shared_library_ids: Vec::new(),
            labels_allow: Vec::new(),
            labels_exclude: Vec::new(),
            age_restriction: None,
        }
    }

    #[test]
    fn koreader_authorized_requires_koreader_sync_role() {
        let plain_user = auth_user_with_roles(&[]);
        let sync_user = auth_user_with_roles(&[AuthUserRole::KoreaderSync]);

        assert!(!koreader_authorized(Some(&plain_user)));
        assert!(koreader_authorized(Some(&sync_user)));
    }

    #[test]
    fn resolve_koreader_user_id_rejects_session_user_without_koreader_sync_role() {
        let plain_user = auth_user_with_roles(&[]);
        let sync_user = auth_user_with_roles(&[AuthUserRole::KoreaderSync]);

        assert_eq!(resolve_koreader_user_id(Some(&plain_user)), None);
        assert_eq!(
            resolve_koreader_user_id(Some(&sync_user)),
            Some("user-1".to_string())
        );
    }

    #[test]
    fn koreader_helpers_return_none_without_sync_scoped_user() {
        assert!(!koreader_authorized(None));
        assert_eq!(resolve_koreader_user_id(None), None);
    }

    #[test]
    fn configured_api_key_identity_uses_configured_metadata_for_matching_token() {
        let identity = configured_api_key_identity(
            "presented",
            Some("presented"),
            Some("api-key-1"),
            Some("KOReader"),
        );

        assert_eq!(
            identity,
            ConfiguredApiKeyIdentity {
                id: "api-key-1".to_string(),
                comment: "KOReader".to_string(),
            }
        );
    }

    #[test]
    fn configured_api_key_identity_returns_unknown_for_non_matching_token() {
        let identity = configured_api_key_identity(
            "presented",
            Some("different"),
            Some("id"),
            Some("comment"),
        );

        assert_eq!(
            identity,
            ConfiguredApiKeyIdentity {
                id: "unknown".to_string(),
                comment: "unknown".to_string(),
            }
        );
    }
}
