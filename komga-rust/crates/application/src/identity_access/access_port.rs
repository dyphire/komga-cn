use async_trait::async_trait;
use serde_json::Value;

use super::kobo_sync::{KoboLibrarySyncRequest, KoboLibrarySyncResponse};
use super::mutation_models::{CreateAuthUserInput, UpdateAuthUserInput, UpdateAuthUserResult};
use super::user_models::{
    AuthOutcome, AuthUser, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity,
};
use crate::identity_access::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
};

#[async_trait]
#[allow(clippy::too_many_arguments)]
pub trait IdentityAccessPort: Send + Sync {
    // --- User authentication ---

    async fn authenticate_basic(&self, username: &str, password: &str) -> Option<AuthOutcome>;

    async fn authenticate_api_key(&self, api_key: &str) -> Option<AuthOutcome>;

    async fn api_key_metadata_by_token(&self, api_key: &str) -> Option<PersistedApiKeyMetadata>;

    // --- User CRUD ---

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

    // --- API keys ---

    async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Option<PersistedApiKey>;

    async fn persisted_api_key_comment_exists(&self, user_id: &str, comment: &str) -> Option<bool>;

    async fn persisted_list_api_keys(&self, user_id: &str) -> Option<Vec<PersistedApiKey>>;

    async fn persisted_delete_api_key_by_id(&self, user_id: &str, api_key_id: &str)
    -> Option<bool>;

    // --- Authentication activity ---

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

    // --- Device auth (Kobo/KOReader) ---

    async fn load_book_created_timestamp(&self, book_id: &str) -> Result<Option<String>, String>;

    async fn load_book_last_epub_position_locator(
        &self,
        book_id: &str,
    ) -> Result<Option<Value>, String>;

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String>;

    async fn load_kobo_library_sync(
        &self,
        request: KoboLibrarySyncRequest,
    ) -> Result<KoboLibrarySyncResponse, String>;

    async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>;

    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, String>;

    async fn load_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, String>;

    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String>;

    async fn persist_read_progress_with_locator(
        &self,
        book_id: &str,
        user_id: &str,
        page: i64,
        completed: bool,
        device_id: &str,
        device_name: &str,
        timestamp: &str,
        locator: Option<Value>,
    ) -> Result<(), String>;

    // --- Session management ---

    fn session_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> String;

    fn remember_me_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> Option<String>;

    fn resolve_session_user(
        &self,
        session_token: Option<&str>,
        remember_me_token: Option<&str>,
    ) -> Option<AuthUser>;

    fn resolve_auth_token(
        &self,
        session_token: Option<&str>,
        remember_me_token: Option<&str>,
    ) -> Option<super::session_tokens::ResolvedAuthToken>;

    fn sync_session_runtime_settings(&self, runtime_key: &str, max_inactive_seconds: u64);

    fn sync_remember_me_runtime_database_file(&self, runtime_key: &str);

    fn sync_remember_me_runtime_settings(&self, runtime_key: &str, key: &str, duration_days: u64);

    fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64;

    fn invalidate_user_sessions(&self, user_id: &str);

    fn invalidate_user_sessions_with_runtime_key(&self, user_id: &str, runtime_key: &str);

    fn invalidate_session_token(&self, token: &str);

    fn invalidate_remember_me_token(&self, token: &str);

    // --- OAuth2 state ---

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
