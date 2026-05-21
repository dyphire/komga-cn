use super::super::session_tokens::ResolvedAuthToken;
use super::super::user_models::AuthUser;

pub trait SessionResolverPort: Send + Sync {
    fn resolve_session_user(
        &self,
        session_token: Option<&str>,
        remember_me_token: Option<&str>,
    ) -> Option<AuthUser>;

    fn resolve_auth_token(
        &self,
        session_token: Option<&str>,
        remember_me_token: Option<&str>,
    ) -> Option<ResolvedAuthToken>;
}
