use super::super::mutation_models::{
    CreateAuthUserInput, UpdateAuthUserInput, UpdateAuthUserResult,
};
use super::super::user_models::{AuthUser, PersistedApiKey};

#[async_trait::async_trait]
pub trait UserAdminPort: Send + Sync {
    async fn persisted_users(&self) -> anyhow::Result<Vec<AuthUser>>;

    async fn create_auth_user(
        &self,
        input: CreateAuthUserInput,
    ) -> anyhow::Result<Option<AuthUser>>;

    async fn delete_auth_user(&self, target_user_id: &str) -> anyhow::Result<bool>;

    async fn update_auth_user(
        &self,
        target_user_id: &str,
        patch: UpdateAuthUserInput,
    ) -> anyhow::Result<UpdateAuthUserResult>;

    async fn persisted_update_password_by_user_id(
        &self,
        user_id: &str,
        password: &str,
    ) -> anyhow::Result<bool>;

    async fn ensure_oauth_user(
        &self,
        email: &str,
        allow_create: bool,
    ) -> anyhow::Result<Option<AuthUser>>;

    async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> anyhow::Result<PersistedApiKey>;

    async fn persisted_api_key_comment_exists(
        &self,
        user_id: &str,
        comment: &str,
    ) -> anyhow::Result<bool>;

    async fn persisted_list_api_keys(&self, user_id: &str) -> anyhow::Result<Vec<PersistedApiKey>>;

    async fn persisted_delete_api_key_by_id(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> anyhow::Result<bool>;
}
