mod auth_session;
#[cfg(test)]
mod auth_session_tests;
mod device_progress;
#[cfg(test)]
mod device_progress_tests;
mod device_records;
#[cfg(test)]
mod device_records_tests;
mod device_tokens;
mod kobo_sync;
mod mutation_models;
mod ports;
mod principal_resolution;
mod session_tokens;
mod user_models;

pub use auth_session::{
    AuthSessionActivityContext, AuthSessionError, AuthSessionRequest, AuthSessionResponseMode,
    AuthSessionService, AuthSessionSuccess, AuthTokenRequest, BasicAuthCredentials,
};
pub use device_progress::{
    DeviceProgressError, DeviceProgressPageCountPort, DeviceProgressReaderPort,
    DeviceProgressService, KoboReadingStateLocationSnapshot, KoboReadingStateSnapshot,
    KoboReadingStateStatus, KoboReadingStateUpdate, KoreaderProgressSnapshot,
    KoreaderProgressUpdate,
};
pub use device_records::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedReadProgressRecord,
    kobo_metadata_pre_paginated,
};
pub use ports::{
    AuthActivityPort, AuthenticationActivityApiKey, AuthenticationPort, DeviceSyncPort,
    DeviceThumbnailBinary, SessionLifecyclePort, SessionResolverPort, UserAdminPort,
};

pub use device_tokens::{
    GeneratedKoboDeviceTokens, generate_kobo_device_tokens, generated_kobo_api_token,
    random_uuid_like, sanitize_identifier,
};
pub use kobo_sync::{
    KOBO_SYNC_ITEM_LIMIT, KoboLibrarySyncRequest, KoboLibrarySyncResponse, KoboLibrarySyncService,
    KoboProxyHeader, KoboProxyPort, KoboProxyRequest, KoboProxyRequestBodyError, KoboProxyResponse,
    KoboStoreSyncMergeResult, KoboStoreSyncPort, KoboSyncAccessPolicy, KoboSyncBookSnapshot,
    KoboSyncBookState, KoboSyncEvent, KoboSyncPage, KoboSyncPageRequest, KoboSyncPointBook,
    KoboSyncReadListSnapshot, KoboSyncReadProgressSnapshot, KoboSyncStatePort,
    KomgaSyncTokenPayload, build_kobo_proxy_request, build_komga_sync_token_payload,
    decode_or_passthrough_sync_token, encode_komga_sync_token_payload,
    is_kobo_store_sync_token_candidate, now_sync_marker, parse_komga_sync_token_payload,
};
pub use mutation_models::{
    AuthUserAgeRestrictionInput, CreateAuthUserInput, SharedLibrariesInput, UpdateAuthUserInput,
    UpdateAuthUserResult,
};
pub use principal_resolution::{
    ConfiguredApiKeyIdentity, configured_api_key_identity, koreader_authorized, resolve_kobo_user,
    resolve_koreader_user_id,
};
pub use session_tokens::{
    AuthTokenSource, RememberMeRuntime, ResolvedAuthToken, SessionRuntime,
    invalidate_remember_me_token, invalidate_session_token, invalidate_user_sessions,
    issue_remember_me_token, issue_session_token, resolve_authenticated_token,
    resolve_authenticated_user,
};
pub use user_models::{
    AuthOutcome, AuthUser, AuthUserAgeRestriction, AuthUserAgeRestrictionKind,
    AuthUserAgeRestrictionSnapshot, AuthUserRole, AuthUserSessionSnapshot, PersistedApiKey,
    PersistedApiKeyMetadata, PersistedAuthenticationActivity,
    user_age_restriction_from_persisted_columns, user_from_session_snapshot, user_has_role,
    user_id, user_is_admin, user_persisted_role_names, user_query_restrictions,
    user_response_role_names, user_roles_from_persisted_names, user_session_snapshot,
    user_shared_all_libraries, user_shared_library_ids,
};
