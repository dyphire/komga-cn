use std::sync::Arc;

use async_trait::async_trait;
use komga_application::identity_access::{
    AuthActivityPort, AuthOutcome, AuthUser, AuthenticationActivityApiKey, AuthenticationPort,
    CreateAuthUserInput, DeviceSyncPort, DeviceThumbnailBinary, KoboMetadataRecord, KoboProxyPort,
    KoboProxyRequest, KoboProxyResponse, KoboSyncBookState, KoboSyncPage, KoboSyncPageRequest,
    KoboSyncPointBook, KoboSyncStatePort, KoreaderBookLookupError, KoreaderBookTarget,
    PersistedApiKey, PersistedApiKeyMetadata, PersistedAuthenticationActivity,
    PersistedReadProgressRecord, ResolvedAuthToken, SessionLifecyclePort, SessionResolverPort,
    UpdateAuthUserInput, UpdateAuthUserResult, UserAdminPort, kobo_metadata_pre_paginated,
};
use komga_application::media_assets::EpubNavigationContentPort;

use crate::auth::runtime_identity_access as auth_identity;
use crate::auth::session_store::RememberMeRuntimeSettings;
use crate::auth::{device_auth, kobo_sync};
use crate::database_handle::DatabaseHandle;
use crate::runtime_identity_access::user_mutation;

#[derive(Clone)]
pub struct IdentityAccess {
    db: DatabaseHandle,
    kobo_proxy_base_url: String,
    epub_navigation_content: Arc<dyn EpubNavigationContentPort>,
}

impl IdentityAccess {
    pub fn default_kobo_proxy_base_url() -> &'static str {
        kobo_sync::DEFAULT_KOBO_PROXY_BASE_URL
    }

    pub fn new(
        db: DatabaseHandle,
        epub_navigation_content: Arc<dyn EpubNavigationContentPort>,
    ) -> Self {
        Self::with_kobo_proxy_base_url(
            db,
            Self::default_kobo_proxy_base_url(),
            epub_navigation_content,
        )
    }

    pub fn with_kobo_proxy_base_url(
        db: DatabaseHandle,
        kobo_proxy_base_url: impl Into<String>,
        epub_navigation_content: Arc<dyn EpubNavigationContentPort>,
    ) -> Self {
        Self {
            db,
            kobo_proxy_base_url: kobo_proxy_base_url.into(),
            epub_navigation_content,
        }
    }
}

impl SessionResolverPort for IdentityAccess {
    fn resolve_session_user(
        &self,
        session_token: Option<&str>,
        remember_me_token: Option<&str>,
    ) -> Result<Option<AuthUser>, String> {
        auth_identity::auth_token_user_from_tokens(session_token, remember_me_token)
    }

    fn resolve_auth_token(
        &self,
        session_token: Option<&str>,
        remember_me_token: Option<&str>,
    ) -> Result<Option<ResolvedAuthToken>, String> {
        auth_identity::auth_token_resolution_from_tokens(session_token, remember_me_token)
    }
}

#[async_trait]
impl AuthenticationPort for IdentityAccess {
    async fn authenticate_basic(
        &self,
        username: &str,
        password: &str,
    ) -> Result<AuthOutcome, String> {
        auth_identity::authenticate_basic_credentials(self.db.read_pool(), username, password).await
    }

    async fn authenticate_api_key(&self, api_key: &str) -> Result<AuthOutcome, String> {
        auth_identity::persisted_api_key_user_by_token(api_key, self.db.read_pool()).await
    }

    async fn api_key_metadata_by_token(
        &self,
        api_key: &str,
    ) -> Result<Option<PersistedApiKeyMetadata>, String> {
        auth_identity::persisted_api_key_metadata_by_token(api_key, self.db.read_pool()).await
    }
}

impl SessionLifecyclePort for IdentityAccess {
    fn session_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> String {
        auth_identity::session_token_for_user_with_runtime_key(user, runtime_key)
    }

    fn remember_me_token_for_user(&self, user: &AuthUser, runtime_key: &str) -> Option<String> {
        auth_identity::remember_me_token_for_user_with_runtime_key(user, runtime_key)
    }

    fn sync_session_runtime_settings(&self, runtime_key: &str, max_inactive_seconds: u64) {
        auth_identity::sync_session_runtime_settings(runtime_key, max_inactive_seconds)
    }

    fn sync_remember_me_runtime_database_file(&self, runtime_key: &str) {
        auth_identity::sync_remember_me_runtime_database_file(runtime_key, self.db.database_file())
    }

    fn sync_remember_me_runtime_settings(&self, runtime_key: &str, key: &str, duration_days: u64) {
        auth_identity::sync_remember_me_runtime_settings(
            runtime_key,
            RememberMeRuntimeSettings {
                key: key.to_string(),
                duration_days,
            },
        )
    }

    fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64 {
        auth_identity::remember_me_max_age_seconds(runtime_key)
    }

    fn invalidate_user_sessions(&self, user_id: &str) {
        auth_identity::invalidate_user_sessions(user_id)
    }

    fn invalidate_user_sessions_with_runtime_key(&self, user_id: &str, runtime_key: &str) {
        auth_identity::invalidate_user_sessions_with_runtime_key(user_id, runtime_key)
    }

    fn invalidate_session_token(&self, token: &str) {
        auth_identity::invalidate_session_token(token)
    }

    fn invalidate_remember_me_token(&self, token: &str) {
        auth_identity::invalidate_remember_me_token(token)
    }

    fn store_oauth2_authorization_state(
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

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
    ) -> Option<String> {
        auth_identity::take_oauth2_authorization_state(runtime_key, session_token, registration_id)
    }
}

#[async_trait]
impl UserAdminPort for IdentityAccess {
    async fn persisted_users(&self) -> Result<Vec<AuthUser>, String> {
        auth_identity::persisted_users(self.db.read_pool()).await
    }

    async fn create_auth_user(
        &self,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, String> {
        user_mutation::create_auth_user(self.db.write_pool(), input)
            .await
            .map_err(|e| e.to_string())
    }

    async fn delete_auth_user(&self, target_user_id: &str) -> Result<bool, String> {
        user_mutation::delete_auth_user(self.db.write_pool(), target_user_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn update_auth_user(
        &self,
        target_user_id: &str,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, String> {
        user_mutation::update_auth_user(self.db.write_pool(), target_user_id, patch)
            .await
            .map_err(|e| e.to_string())
    }

    async fn persisted_update_password_by_user_id(
        &self,
        user_id: &str,
        password: &str,
    ) -> Result<bool, String> {
        auth_identity::persisted_update_password_by_user_id(self.db.write_pool(), user_id, password)
            .await
    }

    async fn ensure_oauth_user(
        &self,
        email: &str,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, String> {
        auth_identity::ensure_oauth_user(self.db.write_pool(), email, allow_create)
            .await
            .map_err(|e| e.to_string())
    }

    async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Result<PersistedApiKey, String> {
        auth_identity::persisted_create_api_key(self.db.write_pool(), user_id, comment).await
    }

    async fn persisted_api_key_comment_exists(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Result<bool, String> {
        auth_identity::persisted_api_key_comment_exists(self.db.read_pool(), user_id, comment).await
    }

    async fn persisted_list_api_keys(&self, user_id: &str) -> Result<Vec<PersistedApiKey>, String> {
        auth_identity::persisted_list_api_keys(self.db.read_pool(), user_id).await
    }

    async fn persisted_delete_api_key_by_id(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Result<bool, String> {
        auth_identity::persisted_delete_api_key_by_id(self.db.write_pool(), user_id, api_key_id)
            .await
    }
}

#[async_trait]
impl AuthActivityPort for IdentityAccess {
    async fn persisted_list_authentication_activity(
        &self,
        user_id: Option<&str>,
    ) -> Result<Vec<PersistedAuthenticationActivity>, String> {
        auth_identity::persisted_list_authentication_activity(self.db.read_pool(), user_id).await
    }

    async fn persisted_cleanup_authentication_activity(&self) -> Result<u64, String> {
        auth_identity::persisted_cleanup_authentication_activity(self.db.write_pool()).await
    }

    async fn persisted_record_failed_authentication_activity(
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

    async fn persisted_record_successful_authentication_activity(
        &self,
        user: &AuthUser,
        source: &str,
        api_key: AuthenticationActivityApiKey<'_>,
        ip: Option<&str>,
        user_agent: Option<&str>,
    ) -> Option<()> {
        auth_identity::persisted_record_successful_authentication_activity(
            self.db.write_pool(),
            user,
            source,
            api_key,
            ip,
            user_agent,
        )
        .await
    }
}

#[async_trait]
impl DeviceSyncPort for IdentityAccess {
    async fn load_book_created_timestamp(&self, book_id: &str) -> Result<Option<String>, String> {
        device_auth::load_book_created_timestamp(self.db.read_pool(), book_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, String> {
        let Some(record) = device_auth::load_kobo_metadata_record(self.db.read_pool(), book_id)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };

        Ok(Some(self.kobo_metadata_record(record)?))
    }

    async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        device_auth::load_koreader_book_target(self.db.read_pool(), book_hash).await
    }

    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, String> {
        device_auth::load_read_progress(self.db.read_pool(), book_id, user_id)
            .await
            .map_err(|e| e.to_string())
    }

    async fn load_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<DeviceThumbnailBinary>, String> {
        device_auth::load_thumbnail_by_id(self.db.read_pool(), thumbnail_id).await
    }

    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, String> {
        device_auth::persisted_book_exists(self.db.read_pool(), book_id)
            .await
            .map_err(|e| e.to_string())
    }
}

impl IdentityAccess {
    fn kobo_metadata_record(
        &self,
        record: device_auth::PersistedKoboMetadataRecord,
    ) -> Result<KoboMetadataRecord, String> {
        Ok(KoboMetadataRecord {
            title: record.title,
            summary: record.summary,
            release_date: record.release_date,
            created_date: record.created_date,
            language: record.language,
            file_size: record.file_size,
            file_name: record.file_name,
            media_type: record.media_type,
            contributor_names: record.contributor_names,
            isbn: record.isbn,
            publisher_name: record.publisher_name,
            cover_image_id: record.cover_image_id,
            series_id: record.series_id,
            series_name: record.series_name,
            series_number: record.series_number,
            series_number_float: record.series_number_float,
            oneshot: record.oneshot,
            is_kepub: record.is_kepub,
            is_pre_paginated: kobo_metadata_pre_paginated(
                self.epub_navigation_content.as_ref(),
                record.epub_extension_blob.as_deref(),
            )?,
        })
    }
}

#[async_trait]
impl KoboSyncStatePort for IdentityAccess {
    async fn load_sync_page(&self, request: KoboSyncPageRequest) -> Result<KoboSyncPage, String> {
        kobo_sync::SqliteKoboSyncState::new(self.db.write_pool())
            .load_sync_page(request)
            .await
    }

    async fn load_sync_book_states(
        &self,
        books: &[KoboSyncPointBook],
        user_id: &str,
    ) -> Result<Vec<KoboSyncBookState>, String> {
        kobo_sync::SqliteKoboSyncState::new(self.db.read_pool())
            .load_sync_book_states(books, user_id)
            .await
    }

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), String> {
        kobo_sync::SqliteKoboSyncState::new(self.db.write_pool())
            .remove_sync_point(sync_point_id)
            .await
    }
}

#[async_trait]
impl KoboProxyPort for IdentityAccess {
    async fn proxy_kobo_request(
        &self,
        request: KoboProxyRequest,
    ) -> Result<KoboProxyResponse, String> {
        kobo_sync::proxy_kobo_request(self.kobo_proxy_base_url.as_str(), request).await
    }
}
