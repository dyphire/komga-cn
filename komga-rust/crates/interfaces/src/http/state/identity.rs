use super::*;

#[allow(clippy::too_many_arguments)]
#[async_trait]
pub trait IdentityService: Send + Sync {
    fn auth_token_user(&self, headers: HeaderMap) -> Option<AuthUser>;
    fn session_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> String;
    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> Option<String>;
    fn sync_session_runtime_settings(&self, runtime_key: String, max_inactive_seconds: u64);
    fn sync_remember_me_runtime_database_file(&self, runtime_key: String, database_file: PathBuf);
    fn sync_remember_me_runtime_settings(
        &self,
        runtime_key: String,
        key: String,
        duration_days: u64,
    );
    fn remember_me_max_age_seconds(&self, runtime_key: String) -> u64;
    fn invalidate_user_sessions(&self, user_id: String);
    fn invalidate_user_sessions_with_runtime_key(&self, user_id: String, runtime_key: String);
    fn invalidate_session_token(&self, token: String);
    fn invalidate_remember_me_token(&self, token: String);
    fn store_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
        state: String,
    );
    fn take_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
    ) -> Option<String>;
    async fn persisted_basic_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome>;
    async fn persisted_api_key_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome>;
    async fn persisted_api_key_user_by_token(
        &self,
        api_key: String,
        database_file: PathBuf,
    ) -> Option<AuthOutcome>;
    async fn persisted_api_key_metadata(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<PersistedApiKeyMetadata>;
    async fn persisted_users(&self, database_file: PathBuf) -> Option<Vec<AuthUser>>;
    async fn persisted_update_password_by_user_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        password: String,
    ) -> Option<bool>;
    async fn persisted_create_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<PersistedApiKey>;
    async fn persisted_api_key_comment_exists(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<bool>;
    async fn persisted_list_api_keys(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Option<Vec<PersistedApiKey>>;
    async fn persisted_delete_api_key_by_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<bool>;
    async fn persisted_list_authentication_activity(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Option<Vec<PersistedAuthenticationActivity>>;
    async fn persisted_cleanup_authentication_activity(
        &self,
        database_file: PathBuf,
    ) -> Option<u64>;
    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<PersistedAuthenticationActivity>;
    async fn persisted_record_failed_authentication_activity(
        &self,
        database_file: PathBuf,
        email: Option<String>,
        input: AuthenticationActivityWriteInput,
        error: String,
    ) -> Option<()>;
    async fn persisted_record_successful_authentication_activity(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        input: AuthenticationActivityWriteInput,
    ) -> Option<()>;
    async fn ensure_oauth_user(
        &self,
        database_file: PathBuf,
        email: String,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error>;
    fn configured_api_key(&self) -> Option<String>;
    async fn load_book_created_timestamp(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<String>, sqlx::Error>;
    async fn load_book_last_epub_position_locator(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<Value>, sqlx::Error>;
    async fn load_book_media_file(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error>;
    async fn load_kobo_metadata_record(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error>;
    async fn load_kobo_sync_page(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        user_id: String,
        current_api_key_id: Option<String>,
        ongoing_sync_point_id: Option<String>,
        last_successful_sync_point_id: Option<String>,
        limit: usize,
    ) -> Result<KoboSyncPage, sqlx::Error>;
    async fn load_koreader_book_target(
        &self,
        database_file: PathBuf,
        book_hash: String,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError>;
    async fn load_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error>;
    async fn load_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error>;
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
    ) -> Result<(), String>;
    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, sqlx::Error>;
    async fn proxy_kobo_store_library_sync(
        &self,
        forwarded_headers: Vec<(String, String)>,
        query: Option<String>,
        raw_sync_token: String,
    ) -> Result<KoboStoreSyncMergeResult, ()>;
    async fn remove_sync_point(
        &self,
        database_file: PathBuf,
        sync_point_id: String,
    ) -> Result<(), sqlx::Error>;
    async fn create_auth_user(
        &self,
        database_file: PathBuf,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error>;
    async fn delete_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
    ) -> Result<bool, sqlx::Error>;
    async fn update_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error>;
    async fn open_auth_pool(&self, database_file: PathBuf) -> Result<SqlitePool, sqlx::Error>;
}

#[async_trait]
impl<T> IdentityService for Arc<T>
where
    T: IdentityService + ?Sized,
{
    fn auth_token_user(&self, headers: HeaderMap) -> Option<AuthUser> {
        (**self).auth_token_user(headers)
    }

    fn session_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> String {
        (**self).session_token_for_user_with_runtime_key(user, runtime_key)
    }

    fn remember_me_token_for_user_with_runtime_key(
        &self,
        user: AuthUser,
        runtime_key: String,
    ) -> Option<String> {
        (**self).remember_me_token_for_user_with_runtime_key(user, runtime_key)
    }

    fn sync_session_runtime_settings(&self, runtime_key: String, max_inactive_seconds: u64) {
        (**self).sync_session_runtime_settings(runtime_key, max_inactive_seconds)
    }

    fn sync_remember_me_runtime_database_file(&self, runtime_key: String, database_file: PathBuf) {
        (**self).sync_remember_me_runtime_database_file(runtime_key, database_file)
    }

    fn sync_remember_me_runtime_settings(
        &self,
        runtime_key: String,
        key: String,
        duration_days: u64,
    ) {
        (**self).sync_remember_me_runtime_settings(runtime_key, key, duration_days)
    }

    fn remember_me_max_age_seconds(&self, runtime_key: String) -> u64 {
        (**self).remember_me_max_age_seconds(runtime_key)
    }

    fn invalidate_user_sessions(&self, user_id: String) {
        (**self).invalidate_user_sessions(user_id)
    }

    fn invalidate_user_sessions_with_runtime_key(&self, user_id: String, runtime_key: String) {
        (**self).invalidate_user_sessions_with_runtime_key(user_id, runtime_key)
    }

    fn invalidate_session_token(&self, token: String) {
        (**self).invalidate_session_token(token)
    }

    fn invalidate_remember_me_token(&self, token: String) {
        (**self).invalidate_remember_me_token(token)
    }

    fn store_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
        state: String,
    ) {
        (**self).store_oauth2_authorization_state(
            runtime_key,
            session_token,
            registration_id,
            state,
        )
    }

    fn take_oauth2_authorization_state(
        &self,
        runtime_key: String,
        session_token: String,
        registration_id: String,
    ) -> Option<String> {
        (**self).take_oauth2_authorization_state(runtime_key, session_token, registration_id)
    }

    async fn persisted_basic_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        (**self).persisted_basic_user(headers, database_file).await
    }

    async fn persisted_api_key_user(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        (**self)
            .persisted_api_key_user(headers, database_file)
            .await
    }

    async fn persisted_api_key_user_by_token(
        &self,
        api_key: String,
        database_file: PathBuf,
    ) -> Option<AuthOutcome> {
        (**self)
            .persisted_api_key_user_by_token(api_key, database_file)
            .await
    }

    async fn persisted_api_key_metadata(
        &self,
        headers: HeaderMap,
        database_file: PathBuf,
    ) -> Option<PersistedApiKeyMetadata> {
        (**self)
            .persisted_api_key_metadata(headers, database_file)
            .await
    }

    async fn persisted_users(&self, database_file: PathBuf) -> Option<Vec<AuthUser>> {
        (**self).persisted_users(database_file).await
    }

    async fn persisted_update_password_by_user_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        password: String,
    ) -> Option<bool> {
        (**self)
            .persisted_update_password_by_user_id(database_file, user_id, password)
            .await
    }

    async fn persisted_create_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<PersistedApiKey> {
        (**self)
            .persisted_create_api_key(database_file, user_id, comment)
            .await
    }

    async fn persisted_api_key_comment_exists(
        &self,
        database_file: PathBuf,
        user_id: String,
        comment: String,
    ) -> Option<bool> {
        (**self)
            .persisted_api_key_comment_exists(database_file, user_id, comment)
            .await
    }

    async fn persisted_list_api_keys(
        &self,
        database_file: PathBuf,
        user_id: String,
    ) -> Option<Vec<PersistedApiKey>> {
        (**self)
            .persisted_list_api_keys(database_file, user_id)
            .await
    }

    async fn persisted_delete_api_key_by_id(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<bool> {
        (**self)
            .persisted_delete_api_key_by_id(database_file, user_id, api_key_id)
            .await
    }

    async fn persisted_list_authentication_activity(
        &self,
        database_file: PathBuf,
        user_id: Option<String>,
    ) -> Option<Vec<PersistedAuthenticationActivity>> {
        (**self)
            .persisted_list_authentication_activity(database_file, user_id)
            .await
    }

    async fn persisted_cleanup_authentication_activity(
        &self,
        database_file: PathBuf,
    ) -> Option<u64> {
        (**self)
            .persisted_cleanup_authentication_activity(database_file)
            .await
    }

    async fn persisted_latest_authentication_activity_by_user_and_api_key(
        &self,
        database_file: PathBuf,
        user_id: String,
        api_key_id: String,
    ) -> Option<PersistedAuthenticationActivity> {
        (**self)
            .persisted_latest_authentication_activity_by_user_and_api_key(
                database_file,
                user_id,
                api_key_id,
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
        (**self)
            .persisted_record_failed_authentication_activity(database_file, email, input, error)
            .await
    }

    async fn persisted_record_successful_authentication_activity(
        &self,
        database_file: PathBuf,
        user: AuthUser,
        input: AuthenticationActivityWriteInput,
    ) -> Option<()> {
        (**self)
            .persisted_record_successful_authentication_activity(database_file, user, input)
            .await
    }

    async fn ensure_oauth_user(
        &self,
        database_file: PathBuf,
        email: String,
        allow_create: bool,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        (**self)
            .ensure_oauth_user(database_file, email, allow_create)
            .await
    }

    fn configured_api_key(&self) -> Option<String> {
        (**self).configured_api_key()
    }

    async fn load_book_created_timestamp(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<String>, sqlx::Error> {
        (**self)
            .load_book_created_timestamp(database_file, book_id)
            .await
    }

    async fn load_book_last_epub_position_locator(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<Value>, sqlx::Error> {
        (**self)
            .load_book_last_epub_position_locator(database_file, book_id)
            .await
    }

    async fn load_book_media_file(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<PersistedBookMediaFile>, sqlx::Error> {
        (**self).load_book_media_file(database_file, book_id).await
    }

    async fn load_kobo_metadata_record(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<Option<KoboMetadataRecord>, sqlx::Error> {
        (**self)
            .load_kobo_metadata_record(database_file, book_id)
            .await
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
        (**self)
            .load_kobo_sync_page(
                database_file,
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
        database_file: PathBuf,
        book_hash: String,
    ) -> Result<Option<KoreaderBookTarget>, KoreaderBookLookupError> {
        (**self)
            .load_koreader_book_target(database_file, book_hash)
            .await
    }

    async fn load_read_progress(
        &self,
        database_file: PathBuf,
        book_id: String,
        user_id: String,
    ) -> Result<Option<PersistedReadProgressRecord>, sqlx::Error> {
        (**self)
            .load_read_progress(database_file, book_id, user_id)
            .await
    }

    async fn load_thumbnail_by_id(
        &self,
        database_file: PathBuf,
        thumbnail_id: String,
    ) -> Result<Option<(String, Vec<u8>)>, sqlx::Error> {
        (**self)
            .load_thumbnail_by_id(database_file, thumbnail_id)
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
        (**self)
            .persist_read_progress_with_locator(
                database_file,
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

    async fn persisted_book_exists(
        &self,
        database_file: PathBuf,
        book_id: String,
    ) -> Result<bool, sqlx::Error> {
        (**self).persisted_book_exists(database_file, book_id).await
    }

    async fn proxy_kobo_store_library_sync(
        &self,
        forwarded_headers: Vec<(String, String)>,
        query: Option<String>,
        raw_sync_token: String,
    ) -> Result<KoboStoreSyncMergeResult, ()> {
        (**self)
            .proxy_kobo_store_library_sync(forwarded_headers, query, raw_sync_token)
            .await
    }

    async fn remove_sync_point(
        &self,
        database_file: PathBuf,
        sync_point_id: String,
    ) -> Result<(), sqlx::Error> {
        (**self)
            .remove_sync_point(database_file, sync_point_id)
            .await
    }

    async fn create_auth_user(
        &self,
        database_file: PathBuf,
        input: CreateAuthUserInput,
    ) -> Result<Option<AuthUser>, sqlx::Error> {
        (**self).create_auth_user(database_file, input).await
    }

    async fn delete_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
    ) -> Result<bool, sqlx::Error> {
        (**self)
            .delete_auth_user(database_file, target_user_id)
            .await
    }

    async fn update_auth_user(
        &self,
        database_file: PathBuf,
        target_user_id: String,
        patch: UpdateAuthUserInput,
    ) -> Result<UpdateAuthUserResult, sqlx::Error> {
        (**self)
            .update_auth_user(database_file, target_user_id, patch)
            .await
    }

    async fn open_auth_pool(&self, database_file: PathBuf) -> Result<SqlitePool, sqlx::Error> {
        (**self).open_auth_pool(database_file).await
    }
}
