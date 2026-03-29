use std::path::Path;

use super::user_models::AuthUser;

pub trait SessionTokenStore {
    fn configure_remember_me_store(&self, store_root: &Path) -> String;
    fn issue_session_token(&self, user: &AuthUser, namespace: &str) -> String;
    fn issue_remember_me_token(&self, user: &AuthUser, namespace: &str) -> Option<String>;
    fn resolve_session_user(&self, token: &str) -> Option<AuthUser>;
    fn resolve_remember_me_user(&self, token: &str) -> Option<AuthUser>;
    fn invalidate_user_sessions(&self, target_user_id: &str);
    fn invalidate_session_token(&self, token: &str);
    fn invalidate_remember_me_token(&self, token: &str);
}

pub fn configure_remember_me_store<S>(store: &S, store_root: &Path) -> String
where
    S: SessionTokenStore + ?Sized,
{
    store.configure_remember_me_store(store_root)
}

pub fn resolve_authenticated_user<S>(
    store: &S,
    session_token: Option<&str>,
    remember_me_token: Option<&str>,
) -> Option<AuthUser>
where
    S: SessionTokenStore + ?Sized,
{
    if let Some(token) = session_token.and_then(non_empty_token)
        && let Some(user) = store.resolve_session_user(token)
    {
        return Some(user);
    }

    remember_me_token
        .and_then(non_empty_token)
        .and_then(|token| store.resolve_remember_me_user(token))
}

pub fn issue_session_token<S>(store: &S, user: &AuthUser, namespace: &str) -> String
where
    S: SessionTokenStore + ?Sized,
{
    store.issue_session_token(user, namespace)
}

pub fn issue_remember_me_token<S>(store: &S, user: &AuthUser, namespace: &str) -> Option<String>
where
    S: SessionTokenStore + ?Sized,
{
    let namespace = namespace.trim();
    if namespace.is_empty() {
        return None;
    }

    store.issue_remember_me_token(user, namespace)
}

pub fn invalidate_user_sessions<S>(store: &S, target_user_id: &str)
where
    S: SessionTokenStore + ?Sized,
{
    store.invalidate_user_sessions(target_user_id)
}

pub fn invalidate_session_token<S>(store: &S, token: &str)
where
    S: SessionTokenStore + ?Sized,
{
    store.invalidate_session_token(token)
}

pub fn invalidate_remember_me_token<S>(store: &S, token: &str)
where
    S: SessionTokenStore + ?Sized,
{
    store.invalidate_remember_me_token(token)
}

fn non_empty_token(token: &str) -> Option<&str> {
    let token = token.trim();
    if token.is_empty() { None } else { Some(token) }
}
