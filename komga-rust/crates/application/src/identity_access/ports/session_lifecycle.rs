use super::super::user_models::AuthUser;

pub trait SessionLifecyclePort: Send + Sync {
    fn session_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> String;

    fn remember_me_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> Option<String>;

    fn sync_session_runtime_settings(&self, runtime_key: &str, max_inactive_seconds: u64);

    fn sync_remember_me_runtime_database_file(&self, runtime_key: &str);

    fn sync_remember_me_runtime_settings(&self, runtime_key: &str, key: &str, duration_days: u64);

    fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64;

    fn invalidate_user_sessions(&self, user_id: &str);

    fn invalidate_user_sessions_with_runtime_key(&self, user_id: &str, runtime_key: &str);

    fn invalidate_session_token(&self, token: &str);

    fn invalidate_remember_me_token(&self, token: &str);

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    );

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String>;
}
