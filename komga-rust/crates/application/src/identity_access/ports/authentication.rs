use super::super::user_models::{AuthOutcome, PersistedApiKeyMetadata};

#[async_trait::async_trait]
pub trait AuthenticationPort: Send + Sync {
    async fn authenticate_basic(
        &self,
        username: &str,
        password: &str,
    ) -> anyhow::Result<AuthOutcome>;

    async fn authenticate_api_key(&self, api_key: &str) -> anyhow::Result<AuthOutcome>;

    async fn api_key_metadata_by_token(
        &self,
        api_key: &str,
    ) -> anyhow::Result<Option<PersistedApiKeyMetadata>>;
}
