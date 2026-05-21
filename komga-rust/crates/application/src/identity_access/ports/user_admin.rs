use async_trait::async_trait;

use super::super::mutation_models::{
    CreateAuthUserInput, UpdateAuthUserInput, UpdateAuthUserResult,
};
use super::super::user_models::{AuthUser, PersistedApiKey};

#[async_trait]
pub trait UserAdminPort: Send + Sync {
    async fn persisted_users(&self) -> Option<Vec<AuthUser>>;

    async fn create_auth_user(
        &self,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, String>;

    async fn delete_auth_user(&self, target_user_id: &str) -> Result<bool, String>;

    async fn update_auth_user(
        &self,
        target_user_id: &str,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, String>;

    async fn persisted_update_password_by_user_id(
        &self,
        user_id: &str,
        password: &str,
    ) -> Option<bool>;

    async fn ensure_oauth_user(
        &self,
        email: &str,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, String>;

    async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Option<PersistedApiKey>;

    async fn persisted_api_key_comment_exists(&self, user_id: &str, comment: &str) -> Option<bool>;

    async fn persisted_list_api_keys(&self, user_id: &str) -> Option<Vec<PersistedApiKey>>;

    async fn persisted_delete_api_key_by_id(&self, user_id: &str, api_key_id: &str)
    -> Option<bool>;
}
