use super::*;
use axum::http::HeaderMap;
use komga_application::identity_access::{
    AuthOutcome, AuthUser, KoboStoreSyncMergeResult, KoboSyncPage, PersistedApiKey,
    PersistedApiKeyMetadata, PersistedAuthenticationActivity,
};
use komga_infrastructure::auth::runtime_identity_access as infrastructure_auth_runtime_identity;
use komga_infrastructure::auth::session_store::RememberMeRuntimeSettings;
use komga_infrastructure::runtime_identity_access as infrastructure_runtime_identity_access;
use komga_interfaces::http::state::IdentityService;
use komga_interfaces::runtime_identity_access::{
    AuthenticationActivityWriteInput, CreateAuthUserInput,
    KoboMetadataRecord as InterfacesKoboMetadataRecord,
    KoreaderBookLookupError as InterfacesKoreaderBookLookupError,
    KoreaderBookTarget as InterfacesKoreaderBookTarget,
    PersistedBookMediaFile as InterfacesPersistedBookMediaFile,
    PersistedReadProgressRecord as InterfacesPersistedReadProgressRecord, UpdateAuthUserInput,
    UpdateAuthUserResult,
};
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Clone, Default)]
pub(super) struct RuntimeIdentityService;

pub(super) fn compose_runtime_identity_service() -> Arc<dyn IdentityService> {
    Arc::new(RuntimeIdentityService)
}

#[async_trait::async_trait]
impl IdentityService for RuntimeIdentityService {
    fn auth_token_user(&self, headers: HeaderMap) -> Option<AuthUser> {
        infrastructure_runtime_identity_access::auth_token_user(&headers)
    }

    fn session_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> String {
        infrastructure_runtime_identity_access::session_token_for_user_with_runtime_key(
            &user,
            &runtime_key,
        )
    }

    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> Option<String> {
        infrastructure_runtime_identity_access::remember_me_token_for_user_with_runtime_key(
            &user,
            &runtime_key,
        )
    }

    fn sync_session_runtime_settings(&self, runtime_key: String, max_inactive_seconds: u64) {
        infrastructure_auth_runtime_identity::sync_session_runtime_settings(
            &runtime_key,
            max_inactive_seconds,
        )
    }

    fn sync_remember_me_runtime_database_file(&self, runtime_key: String, database_file: PathBuf) {
        infrastructure_runtime_identity_access::sync_remember_me_runtime_database_file(
            &runtime_key,
            database_file.as_path(),
        )
    }

    fn sync_remember_me_runtime_settings(
        &self,
        runtime_key: String,
        key: String,
        duration_days: u64,
    ) {
        infrastructure_auth_runtime_identity::sync_remember_me_runtime_settings(
            &runtime_key,
            RememberMeRuntimeSettings { key, duration_days },
        )
    }

    fn remember_me_max_age_seconds(&self, runtime_key: String) -> u64 {
        infrastructure_runtime_identity_access::remember_me_max_age_seconds(&runtime_key)
    }

    fn invalidate_user_sessions(&self, user_id: String) {
        infrastructure_runtime_identity_access::invalidate_user_sessions(&user_id)
    }

    fn invalidate_user_sessions_with_runtime_key(&self, user_id: String, runtime_key: String) {
        infrastructure_runtime_identity_access::invalidate_user_sessions_with_runtime_key(
            &user_id,
            &runtime_key,
        )
    }

    fn invalidate_session_token(&self, token: String) {
        infrastructure_runtime_identity_access::invalidate_session_token(&token)
    }

    fn invalidate_remember_me_token(&self, token: String) {
        infrastructure_runtime_identity_access::invalidate_remember_me_token(&token)
    }

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
        state: String,
    ) {
        infrastructure_auth_runtime_identity::store_oauth2_authorization_state(
            &runtime_key,
            &session_token,
            &registration_id,
            &state,
        )
    }

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
    ) -> Option<String> {
        infrastructure_auth_runtime_identity::take_oauth2_authorization_state(
            &runtime_key,
            &session_token,
            &registration_id,
        )
    }

    async fn persisted_basic_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        infrastructure_runtime_identity_access::persisted_basic_user(
            &headers,
            database_file.as_path(),
        )
        .await
    }

    async fn persisted_api_key_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        infrastructure_runtime_identity_access::persisted_api_key_user(
            &headers,
            database_file.as_path(),
        )
        .await
    }

    async fn persisted_api_key_user_by_token(
        &self,
        api_key: String,
        database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        infrastructure_runtime_identity_access::persisted_api_key_user_by_token(
            &api_key,
            database_file.as_path(),
        )
        .await
    }

    async fn persisted_api_key_metadata(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<PersistedApiKeyMetadata> {
        infrastructure_runtime_identity_access::persisted_api_key_metadata(
            &headers,
            database_file.as_path(),
        )
        .await
    }

    async fn persisted_users(&self, database_file: PathBuf) -> Option<Vec<AuthUser>> {
        infrastructure_runtime_identity_access::persisted_users(database_file.as_path()).await
    }

    async fn persisted_update_password_by_user_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        password: String,
    ) -> Option<bool> {
        infrastructure_runtime_identity_access::persisted_update_password_by_user_id(
            database_file.as_path(),
            &user_id,
            &password,
        )
        .await
    }

    async fn persisted_create_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<PersistedApiKey> {
        infrastructure_runtime_identity_access::persisted_create_api_key(
            database_file.as_path(),
            &user_id,
            &comment,
        )
        .await
    }

    async fn persisted_api_key_comment_exists(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<bool> {
        infrastructure_runtime_identity_access::persisted_api_key_comment_exists(
            database_file.as_path(),
            &user_id,
            &comment,
        )
        .await
    }

    async fn persisted_list_api_keys(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Option<Vec<PersistedApiKey>> {
        infrastructure_runtime_identity_access::persisted_list_api_keys(
            database_file.as_path(),
            &user_id,
        )
        .await
    }

    async fn persisted_delete_api_key_by_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<bool> {
        infrastructure_runtime_identity_access::persisted_delete_api_key_by_id(
            database_file.as_path(),
            &user_id,
            &api_key_id,
        )
        .await
    }

    async fn persisted_list_authentication_activity(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Option<Vec<PersistedAuthenticationActivity>> {
        infrastructure_runtime_identity_access::persisted_list_authentication_activity(
            database_file.as_path(),
            user_id.as_deref(),
        )
        .await
    }

    async fn persisted_cleanup_authentication_activity(
        &self,
        database_file: PathBuf,
    ) -> Option<u64> {
        infrastructure_runtime_identity_access::persisted_cleanup_authentication_activity(
            database_file.as_path(),
        )
        .await
    }

    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<PersistedAuthenticationActivity> {
        infrastructure_runtime_identity_access::persisted_latest_authentication_activity_by_user_and_api_key(
            database_file.as_path(),
            &user_id,
            &api_key_id,
        )
        .await
    }

    async fn persisted_record_failed_authentication_activity(
        &self,
        database_file: PathBuf,
        email: Option<String>,
        input: AuthenticationActivityWriteInput,
        error: String,
    ) -> Option<()> {
        infrastructure_auth_runtime_identity::persisted_record_failed_authentication_activity(
            database_file.as_path(),
            email.as_deref(),
            &input.source,
            &error,
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
    }

    async fn persisted_record_successful_authentication_activity(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        input: AuthenticationActivityWriteInput,
    ) -> Option<()> {
        infrastructure_runtime_identity_access::persisted_record_successful_authentication_activity(
            database_file.as_path(),
            &user,
            &input.source,
            input.api_key_id.as_deref(),
            input.api_key_comment.as_deref(),
            input.ip.as_deref(),
            input.user_agent.as_deref(),
        )
        .await
    }

    async fn ensure_oauth_user(
        &self,
        database_file: PathBuf,
        email: String,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        infrastructure_runtime_identity_access::ensure_oauth_user(
            database_file.as_path(),
            &email,
            allow_create,
        )
        .await
    }

    fn configured_api_key(&self) -> Option<String> {
        infrastructure_runtime_identity_access::configured_api_key()
    }

    async fn load_book_created_timestamp(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<String>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_book_created_timestamp(
            database_file.as_path(),
            &book_id,
        )
        .await
    }

    async fn load_book_last_epub_position_locator(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<Value>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_book_last_epub_position_locator(
            database_file.as_path(),
            &book_id,
        )
        .await
    }

    async fn load_book_media_file(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<InterfacesPersistedBookMediaFile>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_book_media_file(
            database_file.as_path(),
            &book_id,
        )
        .await
        .map(|value| value.map(map_persisted_book_media_file))
    }

    async fn load_kobo_metadata_record(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<InterfacesKoboMetadataRecord>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_kobo_metadata_record(
            database_file.as_path(),
            &book_id,
        )
        .await
        .map(|value| value.map(map_kobo_metadata_record))
    }

    async fn load_kobo_sync_page(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        user_id: String,
        current_api_key_id: Option<String>,
        ongoing_sync_point_id: Option<String>,
        last_successful_sync_point_id: Option<String>,
        limit: usize,
    ) -> Result<KoboSyncPage, sqlx::Error> {
        infrastructure_runtime_identity_access::load_kobo_sync_page(
            database_file.as_path(),
            &user,
            &user_id,
            current_api_key_id.as_deref(),
            ongoing_sync_point_id.as_deref(),
            last_successful_sync_point_id.as_deref(),
            limit,
        )
        .await
    }

    async fn load_koreader_book_target(
        &self,
        database_file: PathBuf,
        book_hash: String,
    ) -> Result<Option<InterfacesKoreaderBookTarget>, InterfacesKoreaderBookLookupError> {
        infrastructure_runtime_identity_access::load_koreader_book_target(
            database_file.as_path(),
            &book_hash,
        )
        .await
        .map(|value| value.map(map_koreader_book_target))
        .map_err(map_koreader_lookup_error)
    }

    async fn load_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<InterfacesPersistedReadProgressRecord>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_read_progress(
            database_file.as_path(),
            &book_id,
            &user_id,
        )
        .await
        .map(|value| value.map(map_persisted_read_progress_record))
    }

    async fn load_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
        infrastructure_runtime_identity_access::load_thumbnail_by_id(
            database_file.as_path(),
            &thumbnail_id,
        )
        .await
    }

    async fn persist_read_progress_with_locator(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
        page: i64,
        completed: bool,
        device_id: String,
        device_name: String,
        timestamp: String,
        locator: Option<Value>,
    ) -> Result<(), String> {
        infrastructure_runtime_identity_access::persist_read_progress_with_locator(
            database_file.as_path(),
            &book_id,
            &user_id,
            page,
            completed,
            &device_id,
            &device_name,
            &timestamp,
            locator,
        )
        .await
    }

    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, sqlx::Error> {
        infrastructure_runtime_identity_access::persisted_book_exists(
            database_file.as_path(),
            &book_id,
        )
        .await
    }

    async fn proxy_kobo_store_library_sync(
        &self,
        forwarded_headers: Vec<(String, String)>,
        query: Option<String>,
        raw_sync_token: String,
    ) -> Result<KoboStoreSyncMergeResult, ()> {
        infrastructure_runtime_identity_access::proxy_kobo_store_library_sync(
            &forwarded_headers,
            query.as_deref(),
            &raw_sync_token,
        )
        .await
    }

    async fn remove_sync_point(
        &self,
        database_file: PathBuf,
        sync_point_id: String,
    ) -> Result<(), sqlx::Error> {
        infrastructure_runtime_identity_access::remove_sync_point(
            database_file.as_path(),
            &sync_point_id,
        )
        .await
    }

    async fn create_auth_user(
        &self,
        database_file: PathBuf,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        infrastructure_runtime_identity::create_auth_user(
            database_file.as_path(),
            infrastructure_runtime_identity::CreateAuthUserInput {
                user_id: input.user_id,
                email: input.email,
                password_hash: input.password_hash,
                roles: input.roles,
                shared_libraries: infrastructure_runtime_identity::SharedLibrariesInput {
                    all: input.shared_libraries.all,
                    library_ids: input.shared_libraries.library_ids,
                },
                labels_allow: input.labels_allow,
                labels_exclude: input.labels_exclude,
                age_restriction: input.age_restriction.map(|value| {
                    infrastructure_runtime_identity::AuthUserAgeRestrictionInput {
                        age: value.age,
                        allow_only: value.allow_only,
                    }
                }),
            },
        )
        .await
    }

    async fn delete_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
    ) -> Result<bool, sqlx::Error> {
        infrastructure_runtime_identity::delete_auth_user(database_file.as_path(), &target_user_id)
            .await
    }

    async fn update_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error> {
        infrastructure_runtime_identity::update_auth_user(
            database_file.as_path(),
            &target_user_id,
            infrastructure_runtime_identity::UpdateAuthUserInput {
                roles: patch.roles,
                shared_libraries: patch.shared_libraries.map(|value| {
                    infrastructure_runtime_identity::SharedLibrariesInput {
                        all: value.all,
                        library_ids: value.library_ids,
                    }
                }),
                labels_allow: patch.labels_allow,
                labels_exclude: patch.labels_exclude,
                age_restriction: patch.age_restriction.map(|value| {
                    value.map(|inner| {
                        infrastructure_runtime_identity::AuthUserAgeRestrictionInput {
                            age: inner.age,
                            allow_only: inner.allow_only,
                        }
                    })
                }),
            },
        )
        .await
        .map(|result| UpdateAuthUserResult {
            updated: result.updated,
            expire_sessions: result.expire_sessions,
        })
    }

    async fn open_auth_pool(&self, database_file: PathBuf) -> Result<SqlitePool, sqlx::Error> {
        infrastructure_runtime_identity_access::open_auth_pool(database_file.as_path()).await
    }
}

fn map_persisted_book_media_file(
    record: infrastructure_runtime_identity_access::PersistedBookMediaFile,
) -> InterfacesPersistedBookMediaFile {
    InterfacesPersistedBookMediaFile {
        file_name: record.file_name,
        media_type: record.media_type,
        file_path: record.file_path,
    }
}

fn map_persisted_read_progress_record(
    record: infrastructure_runtime_identity_access::PersistedReadProgressRecord,
) -> InterfacesPersistedReadProgressRecord {
    InterfacesPersistedReadProgressRecord {
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
) -> InterfacesKoreaderBookTarget {
    InterfacesKoreaderBookTarget {
        id: record.id,
        page_count: record.page_count,
        media_type: record.media_type,
    }
}

fn map_kobo_metadata_record(
    record: infrastructure_runtime_identity_access::KoboMetadataRecord,
) -> InterfacesKoboMetadataRecord {
    InterfacesKoboMetadataRecord {
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
) -> InterfacesKoreaderBookLookupError {
    match error {
        infrastructure_runtime_identity_access::KoreaderBookLookupError::Persistence => {
            InterfacesKoreaderBookLookupError::Persistence
        }
        infrastructure_runtime_identity_access::KoreaderBookLookupError::Conflict => {
            InterfacesKoreaderBookLookupError::Conflict
        }
    }
}
