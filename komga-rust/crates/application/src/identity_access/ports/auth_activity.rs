use async_trait::async_trait;

use super::super::user_models::{AuthUser, PersistedAuthenticationActivity};

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait AuthActivityPort: Send + Sync {
    async fn persisted_list_authentication_activity(
        &self,
        user_id: Option<&str>,
    ) -> Option<Vec<PersistedAuthenticationActivity>>;

    async fn persisted_cleanup_authentication_activity(&self) -> Option<u64>;

    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Option<PersistedAuthenticationActivity>;

    async fn persisted_record_failed_authentication_activity(
        &self,
        email: Option<&str>,
        source: &str,
        error: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Option<()>;

    async fn persisted_record_successful_authentication_activity(
        &self,
        user: &AuthUser,
        source: &str,
        api_key_id: Option<&str>,
        api_key_comment: Option<&str>,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Option<()>;
}
