use super::super::user_models::{
    AuthUser, PersistedApiKeyMetadata, PersistedAuthenticationActivity,
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AuthenticationActivityApiKey<'a> {
    pub id: Option<&'a str>,
    pub comment: Option<&'a str>,
}

impl<'a> AuthenticationActivityApiKey<'a> {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn from_persisted(metadata: Option<&'a PersistedApiKeyMetadata>) -> Self {
        metadata
            .map(|metadata| Self {
                id: Some(metadata.id()),
                comment: Some(metadata.comment()),
            })
            .unwrap_or_default()
    }
}

#[async_trait::async_trait]
pub trait AuthActivityPort: Send + Sync {
    async fn persisted_list_authentication_activity(
        &self,
        user_id: Option<&str>,
    ) -> anyhow::Result<Vec<PersistedAuthenticationActivity>>;

    async fn persisted_cleanup_authentication_activity(&self) -> anyhow::Result<u64>;

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
        api_key: AuthenticationActivityApiKey<'_>,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Option<()>;
}
