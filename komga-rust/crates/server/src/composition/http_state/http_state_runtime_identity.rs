use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, CreateAuthUserInput, KoboStoreSyncMergeResult, KoboSyncPage,
    PersistedApiKey, PersistedApiKeyMetadata, PersistedAuthenticationActivity, ResolvedAuthToken,
    UpdateAuthUserInput, UpdateAuthUserResult,
};
use komga_infrastructure::auth::runtime_identity_access as infrastructure_auth_runtime_identity;
use komga_infrastructure::auth::session_store::RememberMeRuntimeSettings;
use komga_infrastructure::database_handle::DatabaseHandle;
use komga_infrastructure::runtime_identity_access as infrastructure_runtime_identity_access;
use komga_interfaces::state::{
    ApiKeyService, AuthActivityService, AuthTokenService, AuthenticationActivityWriteInput,
    DeviceSyncService, KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget,
    PersistedBookMediaFile, PersistedReadProgressRecord, UserManagementService,
};
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Clone)]
pub(crate) struct RuntimeIdentityService {
    db: DatabaseHandle,
}

pub(super) struct IdentityServiceArcs {
    pub auth_token: std::sync::Arc<dyn AuthTokenService>,
    pub user_management: std::sync::Arc<dyn UserManagementService>,
    pub api_key: std::sync::Arc<dyn ApiKeyService>,
    pub auth_activity: std::sync::Arc<dyn AuthActivityService>,
    pub device_sync: std::sync::Arc<dyn DeviceSyncService>,
}

pub(super) fn compose_identity_services(db: DatabaseHandle) -> IdentityServiceArcs {
    let service = std::sync::Arc::new(RuntimeIdentityService { db });
    IdentityServiceArcs {
        auth_token: service.clone(),
        user_management: service.clone(),
        api_key: service.clone(),
        auth_activity: service.clone(),
        device_sync: service,
    }
}

#[async_trait::async_trait]
impl AuthTokenService for RuntimeIdentityService {
    fn auth_token_user(&self, headers: &HeaderMap) -> Option<AuthUser> {
        infrastructure_runtime_identity_access::auth_token_user(headers)
    }

    fn auth_token_resolution(&self, headers: &HeaderMap) -> Option<ResolvedAuthToken> {
        infrastructure_runtime_identity_access::auth_token_resolution(headers)
    }

    fn session_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> String {
        infrastructure_runtime_identity_access::session_token_for_user_with_runtime_key(
            user,
            runtime_key,
        )
    }

    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: &AuthUser,
        runtime_key: &str,
    ) -> Option<String> {
        infrastructure_runtime_identity_access::remember_me_token_for_user_with_runtime_key(
            user,
            runtime_key,
        )
    }

    fn sync_session_runtime_settings(&self, runtime_key: &str, max_inactive_seconds: u64) {
        infrastructure_auth_runtime_identity::sync_session_runtime_settings(
            runtime_key,
            max_inactive_seconds,
        )
    }

    fn sync_remember_me_runtime_database_file(&self, runtime_key: &str) {
        infrastructure_runtime_identity_access::sync_remember_me_runtime_database_file(
            runtime_key,
            self.db.database_file(),
        )
    }

    fn sync_remember_me_runtime_settings(&self, runtime_key: &str, key: &str, duration_days: u64) {
        infrastructure_auth_runtime_identity::sync_remember_me_runtime_settings(
            runtime_key,
            RememberMeRuntimeSettings {
                key: key.to_string(),
                duration_days,
            },
        )
    }

    fn remember_me_max_age_seconds(&self, runtime_key: &str) -> u64 {
        infrastructure_runtime_identity_access::remember_me_max_age_seconds(runtime_key)
    }

    fn invalidate_user_sessions(&self, user_id: &str) {
        infrastructure_runtime_identity_access::invalidate_user_sessions(user_id)
    }

    fn invalidate_user_sessions_with_runtime_key(&self, user_id: &str, runtime_key: &str) {
        infrastructure_runtime_identity_access::invalidate_user_sessions_with_runtime_key(
            user_id,
            runtime_key,
        )
    }

    fn invalidate_session_token(&self, token: &str) {
        infrastructure_runtime_identity_access::invalidate_session_token(token)
    }

    fn invalidate_remember_me_token(&self, token: &str) {
        infrastructure_runtime_identity_access::invalidate_remember_me_token(token)
    }

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: &str,
        session_token: &str,
        registration_id: &str,
        state: &str,
    ) {
        infrastructure_auth_runtime_identity::store_oauth2_authorization_state(
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
        infrastructure_auth_runtime_identity::take_oauth2_authorization_state(
            runtime_key,
            session_token,
            registration_id,
        )
    }
}

#[async_trait::async_trait]
impl UserManagementService for RuntimeIdentityService {
    async fn persisted_basic_user(&self, headers: &HeaderMap) -> Option<AuthOutcome> {
        infrastructure_runtime_identity_access::persisted_basic_user(headers, self.db.read_pool())
            .await
    }

    async fn persisted_api_key_user(&self, headers: &HeaderMap) -> Option<AuthOutcome> {
        infrastructure_runtime_identity_access::persisted_api_key_user(headers, self.db.read_pool())
            .await
    }

    async fn persisted_api_key_user_by_token(&self, api_key: &str) -> Option<AuthOutcome> {
        infrastructure_runtime_identity_access::persisted_api_key_user_by_token(
            api_key,
            self.db.read_pool(),
        )
        .await
    }

    async fn persisted_api_key_metadata(
        &self,
        headers: &HeaderMap,
    ) -> Option<PersistedApiKeyMetadata> {
        infrastructure_runtime_identity_access::persisted_api_key_metadata(
            headers,
            self.db.read_pool(),
        )
        .await
    }

    async fn persisted_users(&self) -> Option<Vec<AuthUser>> {
        infrastructure_runtime_identity_access::persisted_users(self.db.read_pool()).await
    }

    async fn persisted_update_password_by_user_id(
        &self,
        user_id: &str,
        password: &str,
    ) -> Option<bool> {
        infrastructure_runtime_identity_access::persisted_update_password_by_user_id(
            self.db.write_pool(),
            user_id,
            password,
        )
        .await
    }

    async fn ensure_oauth_user(
        &self,
        email: &str,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        infrastructure_runtime_identity_access::ensure_oauth_user(
            self.db.write_pool(),
            email,
            allow_create,
        )
        .await
    }

    fn configured_api_key(&self) -> Option<String> {
        infrastructure_runtime_identity_access::configured_api_key()
    }

    async fn create_auth_user(
        &self,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        infrastructure_runtime_identity_access::create_auth_user(self.db.write_pool(), input).await
    }

    async fn delete_auth_user(&self, target_user_id: &str) -> Result<bool, sqlx::Error> {
        infrastructure_runtime_identity_access::delete_auth_user(
            self.db.write_pool(),
            target_user_id,
        )
        .await
    }

    async fn update_auth_user(
        &self,
        target_user_id: &str,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error> {
        infrastructure_runtime_identity_access::update_auth_user(
            self.db.write_pool(),
            target_user_id,
            patch,
        )
        .await
    }
}

#[async_trait::async_trait]
impl ApiKeyService for RuntimeIdentityService {
    async fn persisted_create_api_key(
        &self,
        user_id: &str,
        comment: &str,
    ) -> Option<PersistedApiKey> {
        infrastructure_runtime_identity_access::persisted_create_api_key(
            self.db.write_pool(),
            user_id,
            comment,
        )
        .await
    }

    async fn persisted_api_key_comment_exists(&self, user_id: &str, comment: &str) -> Option<bool> {
        infrastructure_runtime_identity_access::persisted_api_key_comment_exists(
            self.db.read_pool(),
            user_id,
            comment,
        )
        .await
    }

    async fn persisted_list_api_keys(&self, user_id: &str) -> Option<Vec<PersistedApiKey>> {
        infrastructure_runtime_identity_access::persisted_list_api_keys(
            self.db.read_pool(),
            user_id,
        )
        .await
    }

    async fn persisted_delete_api_key_by_id(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Option<bool> {
        infrastructure_runtime_identity_access::persisted_delete_api_key_by_id(
            self.db.write_pool(),
            user_id,
            api_key_id,
        )
        .await
    }
}

#[async_trait::async_trait]
impl AuthActivityService for RuntimeIdentityService {
    async fn persisted_list_authentication_activity(
        &self,
        user_id: Option<&str>,
    ) -> Option<Vec<PersistedAuthenticationActivity>> {
        infrastructure_runtime_identity_access::persisted_list_authentication_activity(
            self.db.read_pool(),
            user_id,
        )
        .await
    }

    async fn persisted_cleanup_authentication_activity(&self) -> Option<u64> {
        infrastructure_runtime_identity_access::persisted_cleanup_authentication_activity(
            self.db.write_pool(),
        )
        .await
    }

    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        user_id: &str,
        api_key_id: &str,
    ) -> Option<PersistedAuthenticationActivity> {
        infrastructure_runtime_identity_access::persisted_latest_authentication_activity_by_user_and_api_key(
            self.db.read_pool(),
            user_id,
            api_key_id,
        )
        .await
    }

    async fn persisted_record_failed_authentication_activity(
        &self,
        email: Option<&str>,
        input: AuthenticationActivityWriteInput,
        error: &str,
    ) -> Option<()> {
        infrastructure_auth_runtime_identity::persisted_record_failed_authentication_activity(
            self.db.write_pool(),
            email,
            &input.source,
            error,
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
    }

    async fn persisted_record_successful_authentication_activity(
        &self,
        user: &AuthUser,
        input: AuthenticationActivityWriteInput,
    ) -> Option<()> {
        infrastructure_runtime_identity_access::persisted_record_successful_authentication_activity(
            self.db.write_pool(),
            user,
            &input.source,
            input.api_key_id.as_deref(),
            input.api_key_comment.as_deref(),
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
    }
}

#[async_trait::async_trait]
impl DeviceSyncService for RuntimeIdentityService {
    async fn load_book_created_timestamp(
        &self,
        book_id: &str,
    ) -> Result<Option<String>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_book_created_timestamp(
            self.db.read_pool(),
            book_id,
        )
        .await
    }

    async fn load_book_last_epub_position_locator(
        &self,
        book_id: &str,
    ) -> Result<Option<Value>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_book_last_epub_position_locator(
            self.db.read_pool(),
            book_id,
        )
        .await
    }

    async fn load_book_media_file(
        &self,
        book_id: &str,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_book_media_file(self.db.read_pool(), book_id)
            .await
            .map(|value| value.map(map_persisted_book_media_file))
    }

    async fn load_kobo_metadata_record(
        &self,
        book_id: &str,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_kobo_metadata_record(
            self.db.read_pool(),
            book_id,
        )
        .await
        .map(|value| value.map(map_kobo_metadata_record))
    }

    async fn load_kobo_sync_page(
        &self,
        user: &AuthUser,
        user_id: &str,
        current_api_key_id: Option<&str>,
        ongoing_sync_point_id: Option<&str>,
        last_successful_sync_point_id: Option<&str>,
        limit: usize,
    ) -> Result<KoboSyncPage, sqlx::Error> {
        infrastructure_runtime_identity_access::load_kobo_sync_page(
            self.db.write_pool(),
            user,
            user_id,
            current_api_key_id,
            ongoing_sync_point_id,
            last_successful_sync_point_id,
            limit,
        )
        .await
    }

    async fn load_koreader_book_target(
        &self,
        book_hash: &str,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        infrastructure_runtime_identity_access::load_koreader_book_target(
            self.db.read_pool(),
            book_hash,
        )
        .await
        .map(|value| value.map(map_koreader_book_target))
        .map_err(map_koreader_lookup_error)
    }

    async fn load_read_progress(
        &self,
        book_id: &str,
        user_id: &str,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_read_progress(
            self.db.read_pool(),
            book_id,
            user_id,
        )
        .await
        .map(|value| value.map(map_persisted_read_progress_record))
    }

    async fn load_thumbnail_by_id(
        &self,
        thumbnail_id: &str,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_thumbnail_by_id(
            self.db.read_pool(),
            thumbnail_id,
        )
        .await
    }

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
    ) -> Result<(), String> {
        infrastructure_runtime_identity_access::persist_read_progress_with_locator(
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

    async fn persisted_book_exists(&self, book_id: &str) -> Result<bool, sqlx::Error> {
        infrastructure_runtime_identity_access::persisted_book_exists(self.db.read_pool(), book_id)
            .await
    }

    async fn proxy_kobo_store_library_sync(
        &self,
        forwarded_headers: &[(String, String)],
        query: Option<&str>,
        raw_sync_token: &str,
    ) -> Result<KoboStoreSyncMergeResult, ()> {
        infrastructure_runtime_identity_access::proxy_kobo_store_library_sync(
            forwarded_headers,
            query,
            raw_sync_token,
        )
        .await
    }

    async fn remove_sync_point(&self, sync_point_id: &str) -> Result<(), sqlx::Error> {
        infrastructure_runtime_identity_access::remove_sync_point(
            self.db.write_pool(),
            sync_point_id,
        )
        .await
    }

    async fn open_auth_pool(&self) -> Result<SqlitePool, sqlx::Error> {
        infrastructure_runtime_identity_access::open_auth_pool(self.db.database_file()).await
    }
}

fn map_persisted_book_media_file(
    record: infrastructure_runtime_identity_access::PersistedBookMediaFile,
) -> PersistedBookMediaFile {
    PersistedBookMediaFile {
        file_name: record.file_name,
        media_type: record.media_type,
        file_path: record.file_path,
    }
}

fn map_persisted_read_progress_record(
    record: infrastructure_runtime_identity_access::PersistedReadProgressRecord,
) -> PersistedReadProgressRecord {
    PersistedReadProgressRecord {
        page: record.page,
        completed: record.completed,
        created: record.created,
        last_modified: record.last_modified,
        device_id: record.device_id,
        device_name: record.device_name,
        locator: record.locator,
    }
}

fn map_koreader_book_target(
    record: infrastructure_runtime_identity_access::KoreaderBookTarget,
) -> KoreaderBookTarget {
    KoreaderBookTarget {
        id: record.id,
        page_count: record.page_count,
        media_type: record.media_type,
    }
}

fn map_kobo_metadata_record(
    record: infrastructure_runtime_identity_access::KoboMetadataRecord,
) -> KoboMetadataRecord {
    KoboMetadataRecord {
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
        is_pre_paginated: record.is_pre_paginated,
    }
}

fn map_koreader_lookup_error(
    error: infrastructure_runtime_identity_access::KoreaderBookLookupError,
) -> KoreaderBookLookupError {
    match error {
        infrastructure_runtime_identity_access::KoreaderBookLookupError::Persistence => {
            KoreaderBookLookupError::Persistence
        }
        infrastructure_runtime_identity_access::KoreaderBookLookupError::Conflict => {
            KoreaderBookLookupError::Conflict
        }
    }
}
