use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, CreateAuthUserInput, KoboLibrarySyncRequest, KoboLibrarySyncResponse,
    PersistedApiKey, PersistedApiKeyMetadata, PersistedAuthenticationActivity, ResolvedAuthToken,
    UpdateAuthUserInput, UpdateAuthUserResult,
};
use serde_json::Value;
use sqlx::SqlitePool;

use crate::auth::runtime_identity_access as auth_identity;
use crate::auth::session_store::RememberMeRuntimeSettings;
use crate::auth::{device_auth, device_auth_config, kobo_sync};
use crate::database_handle::DatabaseHandle;
use crate::runtime_identity_access::user_mutation;

pub use crate::auth::device_auth::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord,
};

#[derive(Clone)]
pub struct IdentityAccess {
    db: DatabaseHandle,
}

impl IdentityAccess {
    pub fn new(db: DatabaseHandle) -> Self {
        Self { db }
    }

    pub fn database_file(&self) -> &std::path::Path {
        self.db.database_file()
    }

    pub fn auth_token_user(&self, headers: &HeaderMap) -> Option<AuthUser> {
        auth_identity::auth_token_user(headers)
    }

    pub fn auth_token_resolution(&self, headers: &HeaderMap) -> Option<ResolvedAuthToken> {
        auth_identity::auth_token_resolution(headers)
    }

    pub fn session_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> String {
        auth_identity::session_token_for_user_with_runtime_key(user, runtime_key)
    }

    pub fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> Option<String> {
        auth_identity::remember_me_token_for_user_with_runtime_key(user, runtime_key)
    }

    pub fn sync_session_runtime_settings(&self, runtime_key: &str, max_inactive_seconds: u64) {
        auth_identity::sync_session_runtime_settings(runtime_key, max_inactive_seconds)
    }

    pub fn sync_remember_me_runtime_database_file(&self, runtime_key: &str) {
        auth_identity::sync_remember_me_runtime_database_file(runtime_key, self.db.database_file())
    }

    pub fn sync_remember_me_runtime_settings(
        &self,
        runtime_key: &str,
        key: &str,
        duration_days: u64,
    ) {
        auth_identity::sync_remember_me_runtime_settings(
            runtime_key,
            RememberMeRuntimeSettings {
                key: key.to_string(),
                duration_days,
            },
        )
    }

    pub fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64 {
        auth_identity::remember_me_max_age_seconds(runtime_key)
    }

    pub fn invalidate_user_sessions(&self, user_id: &str) {
        auth_identity::invalidate_user_sessions(user_id)
    }

    pub fn invalidate_user_sessions_with_runtime_key(&self, user_id: &str, runtime_key: &str) {
        auth_identity::invalidate_user_sessions_with_runtime_key(user_id, runtime_key)
    }

    pub fn invalidate_session_token(&self, token: &str) {
        auth_identity::invalidate_session_token(token)
    }

    pub fn invalidate_remember_me_token(&self, token: &str) {
        auth_identity::invalidate_remember_me_token(token)
    }

    pub fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    ) {
        auth_identity::store_oauth2_authorization_state(
            runtime_key,
            session_token,
            registration_id,
            state,
        )
    }

    pub fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String> {
        auth_identity::take_oauth2_authorization_state(runtime_key, session_token, registration_id)
    }

    pub async fn persisted_basic_user(&self, headers: &HeaderMap) -> Option<AuthOutcome> {
        auth_identity::persisted_basic_user(headers, self.db.read_pool()).await
    }

    pub async fn persisted_api_key_user(&self, headers: &HeaderMap) -> Option<AuthOutcome> {
        auth_identity::persisted_api_key_user(headers, self.db.read_pool()).await
    }

    pub async fn persisted_api_key_user_by_token(&self, api_key: &str) -> Option<AuthOutcome> {
        auth_identity::persisted_api_key_user_by_token(api_key, self.db.read_pool()).await
    }

    pub async fn persisted_api_key_metadata(
        &self,
        headers: &HeaderMap,
    ) -> Option<PersistedApiKeyMetadata> {
        auth_identity::persisted_api_key_metadata(headers, self.db.read_pool()).await
    }

    pub async fn persisted_users(&self) -> Option<Vec<AuthUser>> {
        auth_identity::persisted_users(self.db.read_pool()).await
    }

    pub async fn persisted_update_password_by_user_id(
        &self,
        user_id: &str,
        password: &str,
    ) -> Option<bool> {
        auth_identity::persisted_update_password_by_user_id(self.db.write_pool(), user_id, password)
            .await
    }

    pub async fn ensure_oauth_user(
        &self,
        email: &str,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        auth_identity::ensure_oauth_user(self.db.write_pool(), email, allow_create).await
    }

    pub fn configured_api_key(&self) -> Option<String> {
        device_auth_config::configured_api_key()
    }

    pub async fn create_auth_user(
        &self,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        user_mutation::create_auth_user(self.db.write_pool(), input).await
    }

    pub async fn delete_auth_user(&self, target_user_id: &str) -> Result<bool, sqlx::Error> {
        user_mutation::delete_auth_user(self.db.write_pool(), target_user_id).await
    }

    pub async fn update_auth_user(
        &self,
        target_user_id: &str,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error> {
        user_mutation::update_auth_user(self.db.write_pool(), target_user_id, patch).await
    }

    pub async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Option<PersistedApiKey> {
        auth_identity::persisted_create_api_key(self.db.write_pool(), user_id, comment).await
    }

    pub async fn persisted_api_key_comment_exists(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Option<bool> {
        auth_identity::persisted_api_key_comment_exists(self.db.read_pool(), user_id, comment).await
    }

    pub async fn persisted_list_api_keys(&self, user_id: &str) -> Option<Vec<PersistedApiKey>> {
        auth_identity::persisted_list_api_keys(self.db.read_pool(), user_id).await
    }

    pub async fn persisted_delete_api_key_by_id(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Option<bool> {
        auth_identity::persisted_delete_api_key_by_id(self.db.write_pool(), user_id, api_key_id)
            .await
    }

    pub async fn persisted_list_authentication_activity(
        &self,
        user_id: Option<&str>,
    ) -> Option<Vec<PersistedAuthenticationActivity>> {
        auth_identity::persisted_list_authentication_activity(self.db.read_pool(), user_id).await
    }

    pub async fn persisted_cleanup_authentication_activity(&self) -> Option<u64> {
        auth_identity::persisted_cleanup_authentication_activity(self.db.write_pool()).await
    }

    pub async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Option<PersistedAuthenticationActivity> {
        auth_identity::persisted_latest_authentication_activity_by_user_and_api_key(
            self.db.read_pool(),
            user_id,
            api_key_id,
        )
        .await
    }

    pub async fn persisted_record_failed_authentication_activity(
        &self,
        email: Option<&str>,
        source: &str,
        error: &str,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Option<()> {
        auth_identity::persisted_record_failed_authentication_activity(
            self.db.write_pool(),
            email,
            source,
            error,
            ip,
            user_agent,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn persisted_record_successful_authentication_activity(
        &self,
        user: &AuthUser,
        source: &str,
        api_key_id: Option<&str>,
        api_key_comment: Option<&str>,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Option<()> {
        auth_identity::persisted_record_successful_authentication_activity(
            self.db.write_pool(),
            user,
            source,
            api_key_id,
            api_key_comment,
            ip,
            user_agent,
        )
        .await
    }

    pub async fn load_book_created_timestamp(
        &self,
        book_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        device_auth::load_book_created_timestamp(self.db.read_pool(), book_id).await
    }

    pub async fn load_book_last_epub_position_locator(
        &self,
        book_id: &str,
    ) -> Result<Option<Value>, sqlx::Error> {
        device_auth::load_book_last_epub_position_locator(self.db.read_pool(), book_id).await
    }

    pub async fn load_book_media_file(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
        device_auth::load_book_media_file(self.db.read_pool(), book_id).await
    }

    pub async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
        device_auth::load_kobo_metadata_record(self.db.read_pool(), book_id).await
    }

    pub async fn load_kobo_library_sync(
        &self,
        request: KoboLibrarySyncRequest,
    ) -> Result<KoboLibrarySyncResponse, sqlx::Error> {
        kobo_sync::load_kobo_library_sync(self.db.write_pool(), request).await
    }

    pub async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        device_auth::load_koreader_book_target(self.db.read_pool(), book_hash).await
    }

    pub async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
        device_auth::load_read_progress(self.db.read_pool(), book_id, user_id).await
    }

    pub async fn load_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
        device_auth::load_thumbnail_by_id(self.db.read_pool(), thumbnail_id).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn persist_read_progress_with_locator(
        &self,
        book_id: &str,
        user_id: &str,
        page: i64,
        completed: bool,
        device_id: &str,
        device_name: &str,
        timestamp: &str,
        locator: Option<Value>,
    ) -> Result<(), String> {
        device_auth::persist_read_progress_with_locator(
            self.db.write_pool(),
            book_id,
            user_id,
            page,
            completed,
            device_id,
            device_name,
            timestamp,
            locator,
        )
        .await
    }

    pub async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, sqlx::Error> {
        device_auth::persisted_book_exists(self.db.read_pool(), book_id).await
    }

    pub async fn open_auth_pool(&self) -> Result<SqlitePool, sqlx::Error> {
        auth_identity::open_auth_pool(self.db.database_file()).await
    }
}
