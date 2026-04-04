mod auth_access;
mod backend_contract;
mod backend_state;
mod kobo_access;
mod test_backend;
mod user_mutation;

pub use auth_access::{
    auth_token_user, configure_remember_me_store, configured_api_key, configured_api_key_comment,
    configured_api_key_id, ensure_oauth_user, invalidate_remember_me_token,
    invalidate_session_token, invalidate_user_sessions, open_auth_pool,
    persisted_api_key_comment_exists, persisted_api_key_metadata, persisted_api_key_user,
    persisted_api_key_user_by_token, persisted_basic_user,
    persisted_cleanup_authentication_activity, persisted_create_api_key,
    persisted_delete_api_key_by_id, persisted_latest_authentication_activity_by_user_and_api_key,
    persisted_list_api_keys, persisted_list_authentication_activity,
    persisted_record_successful_authentication_activity, persisted_update_password_by_user_id,
    persisted_users, remember_me_token_for_user_with_namespace,
    session_token_for_user_with_namespace,
};
pub use backend_contract::{
    AuthUserAgeRestrictionInput, BoxFuture, CreateAuthUserInput, KoboMetadataRecord,
    KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord, RuntimeIdentityAccessBackend, SharedLibrariesInput,
    UpdateAuthUserInput, UpdateAuthUserResult,
};
pub use backend_state::install_runtime_identity_access;
pub use kobo_access::{
    load_book_created_timestamp, load_book_last_epub_position_locator, load_book_media_file,
    load_book_page_count, load_kobo_metadata_record, load_kobo_sync_snapshot,
    load_koreader_book_target, load_read_progress, load_sync_point_marker, load_sync_point_state,
    load_thumbnail_by_id, persist_read_progress_with_locator, persisted_book_exists,
    proxy_kobo_store_library_sync, remove_sync_point, save_sync_point,
};
pub use user_mutation::{create_auth_user, delete_auth_user, update_auth_user};
