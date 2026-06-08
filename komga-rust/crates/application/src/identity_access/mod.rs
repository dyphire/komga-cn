mod device_progress;
#[cfg(test)]
mod device_progress_tests;
mod device_records;
mod device_tokens;
mod kobo_sync;
mod mutation_models;
pub mod ports;
mod principal_resolution;
mod session_tokens;
mod user_models;

pub use device_progress::{
    DeviceProgressError, DeviceProgressService, KoboReadingStateUpdate, KoreaderProgressSnapshot,
    KoreaderProgressUpdate,
};
pub use device_records::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
};
pub use ports::{
    AuthActivityPort, AuthenticationPort, DeviceSyncPort, ReadProgressWithLocatorInput,
    SessionLifecyclePort, SessionResolverPort, UserAdminPort,
};

pub use device_tokens::{
    generated_kobo_api_token, generated_kobo_token_triplet, random_uuid_like, sanitize_identifier,
};
pub use kobo_sync::{
    KOBO_SYNC_ITEM_LIMIT, KoboLibrarySyncPayload, KoboLibrarySyncRequest, KoboLibrarySyncResponse,
    KoboLibrarySyncService, KoboStoreSyncMergeResult, KoboStoreSyncPort, KoboSyncAccessPolicy,
    KoboSyncBookSnapshot, KoboSyncPage, KoboSyncPageRequest, KoboSyncPointBook,
    KoboSyncReadListSnapshot, KoboSyncReadProgressSnapshot, KoboSyncStatePort,
    KomgaSyncTokenPayload, build_kobo_book_metadata_payload,
    build_kobo_changed_entitlement_removed, build_kobo_changed_product_metadata,
    build_kobo_changed_reading_state, build_kobo_changed_tag, build_kobo_deleted_tag,
    build_kobo_library_sync_payload, build_kobo_new_entitlement, build_kobo_new_tag,
    build_komga_sync_token_payload, decode_or_passthrough_sync_token,
    is_kobo_store_sync_token_candidate, now_sync_marker, parse_komga_sync_token_payload,
};
pub use mutation_models::{
    AuthUserAgeRestrictionInput, CreateAuthUserInput, SharedLibrariesInput, UpdateAuthUserInput,
    UpdateAuthUserResult,
};
pub use principal_resolution::{
    configured_api_key_identity, koreader_authorized, resolve_kobo_user, resolve_koreader_user_id,
};
pub use session_tokens::{
    AuthTokenSource, RememberMeRuntime, ResolvedAuthToken, SessionRuntime,
    invalidate_remember_me_token, invalidate_session_token, invalidate_user_sessions,
    issue_remember_me_token, issue_session_token, resolve_authenticated_token,
    resolve_authenticated_user,
};
pub use user_models::{
    AuthOutcome, AuthUser, AuthUserAgeRestriction, AuthUserAgeRestrictionSnapshot,
    AuthUserSessionSnapshot, PersistedApiKey, PersistedApiKeyMetadata,
    PersistedAuthenticationActivity, user_from_session_snapshot, user_has_role, user_id,
    user_is_admin, user_payload_json, user_session_snapshot, user_shared_all_libraries,
    user_shared_library_ids,
};
