use std::sync::Arc;

use crate::auth::{
    device_auth, device_auth_config, kobo_sync, runtime_identity_access as infrastructure_auth,
    session_store,
};

use super::backend_contract::{
    KoboMetadataRecord, KoreaderBookLookupError, KoreaderBookTarget, PersistedBookMediaFile,
    PersistedReadProgressRecord, RuntimeIdentityAccessBackend,
};

pub(super) fn compose_test_runtime_identity_access_backend() -> RuntimeIdentityAccessBackend {
    RuntimeIdentityAccessBackend {
        auth_token_user: Arc::new(|headers| infrastructure_auth::auth_token_user(&headers)),
        session_token_for_user_with_runtime_key: Arc::new(|user, runtime_key| {
            infrastructure_auth::session_token_for_user_with_runtime_key(&user, &runtime_key)
        }),
        remember_me_token_for_user_with_runtime_key: Arc::new(|user, runtime_key| {
            infrastructure_auth::remember_me_token_for_user_with_runtime_key(&user, &runtime_key)
        }),
        sync_remember_me_runtime_database_file: Arc::new(|runtime_key, database_file| {
            infrastructure_auth::sync_remember_me_runtime_database_file(
                &runtime_key,
                database_file.as_path(),
            )
        }),
        sync_remember_me_runtime_settings: Arc::new(|runtime_key, key, duration_days| {
            infrastructure_auth::sync_remember_me_runtime_settings(
                &runtime_key,
                session_store::RememberMeRuntimeSettings { key, duration_days },
            )
        }),
        remember_me_max_age_seconds: Arc::new(|runtime_key| {
            infrastructure_auth::remember_me_max_age_seconds(&runtime_key)
        }),
        invalidate_user_sessions: Arc::new(|user_id| {
            infrastructure_auth::invalidate_user_sessions(&user_id)
        }),
        invalidate_user_sessions_with_runtime_key: Arc::new(|user_id, runtime_key| {
            infrastructure_auth::invalidate_user_sessions_with_runtime_key(&user_id, &runtime_key)
        }),
        invalidate_session_token: Arc::new(|token| {
            infrastructure_auth::invalidate_session_token(&token)
        }),
        invalidate_remember_me_token: Arc::new(|token| {
            infrastructure_auth::invalidate_remember_me_token(&token)
        }),
        persisted_basic_user: Arc::new(|headers, database_file| {
            Box::pin(async move {
                infrastructure_auth::persisted_basic_user(&headers, database_file.as_path()).await
            })
        }),
        persisted_api_key_user: Arc::new(|headers, database_file| {
            Box::pin(async move {
                infrastructure_auth::persisted_api_key_user(&headers, database_file.as_path()).await
            })
        }),
        persisted_api_key_user_by_token: Arc::new(|api_key, database_file| {
            Box::pin(async move {
                infrastructure_auth::persisted_api_key_user_by_token(
                    &api_key,
                    database_file.as_path(),
                )
                .await
            })
        }),
        persisted_api_key_metadata: Arc::new(|headers, database_file| {
            Box::pin(async move {
                infrastructure_auth::persisted_api_key_metadata(&headers, database_file.as_path())
                    .await
            })
        }),
        persisted_users: Arc::new(|database_file| {
            Box::pin(
                async move { infrastructure_auth::persisted_users(database_file.as_path()).await },
            )
        }),
        persisted_update_password_by_user_id: Arc::new(|database_file, user_id, password| {
            Box::pin(async move {
                infrastructure_auth::persisted_update_password_by_user_id(
                    database_file.as_path(),
                    &user_id,
                    &password,
                )
                .await
            })
        }),
        persisted_create_api_key: Arc::new(|database_file, user_id, comment| {
            Box::pin(async move {
                infrastructure_auth::persisted_create_api_key(
                    database_file.as_path(),
                    &user_id,
                    &comment,
                )
                .await
            })
        }),
        persisted_api_key_comment_exists: Arc::new(|database_file, user_id, comment| {
            Box::pin(async move {
                infrastructure_auth::persisted_api_key_comment_exists(
                    database_file.as_path(),
                    &user_id,
                    &comment,
                )
                .await
            })
        }),
        persisted_list_api_keys: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                infrastructure_auth::persisted_list_api_keys(database_file.as_path(), &user_id)
                    .await
            })
        }),
        persisted_delete_api_key_by_id: Arc::new(|database_file, user_id, api_key_id| {
            Box::pin(async move {
                infrastructure_auth::persisted_delete_api_key_by_id(
                    database_file.as_path(),
                    &user_id,
                    &api_key_id,
                )
                .await
            })
        }),
        persisted_list_authentication_activity: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                infrastructure_auth::persisted_list_authentication_activity(
                    database_file.as_path(),
                    user_id.as_deref(),
                )
                .await
            })
        }),
        persisted_cleanup_authentication_activity: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_auth::persisted_cleanup_authentication_activity(
                    database_file.as_path(),
                )
                .await
            })
        }),
        persisted_latest_authentication_activity_by_user_and_api_key: Arc::new(
            |database_file, user_id, api_key_id| {
                Box::pin(async move {
                    infrastructure_auth::persisted_latest_authentication_activity_by_user_and_api_key(
                        database_file.as_path(),
                        &user_id,
                        &api_key_id,
                    )
                    .await
                })
            },
        ),
        persisted_record_successful_authentication_activity: Arc::new(
            |database_file, user, source, api_key_id, api_key_comment, ip, user_agent| {
                Box::pin(async move {
                    infrastructure_auth::persisted_record_successful_authentication_activity(
                        database_file.as_path(),
                        &user,
                        &source,
                        api_key_id.as_deref(),
                        api_key_comment.as_deref(),
                        ip.as_deref(),
                        user_agent.as_deref(),
                    )
                    .await
                })
            },
        ),
        ensure_oauth_user: Arc::new(|database_file, email, allow_create| {
            Box::pin(async move {
                infrastructure_auth::ensure_oauth_user(
                    database_file.as_path(),
                    &email,
                    allow_create,
                )
                .await
            })
        }),
        configured_api_key: Arc::new(device_auth_config::configured_api_key),
        load_book_created_timestamp: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                device_auth::load_book_created_timestamp(database_file.as_path(), &book_id).await
            })
        }),
        load_book_last_epub_position_locator: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                device_auth::load_book_last_epub_position_locator(database_file.as_path(), &book_id)
                    .await
            })
        }),
        load_book_media_file: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                device_auth::load_book_media_file(database_file.as_path(), &book_id)
                    .await
                    .map(|value| value.map(map_test_persisted_book_media_file))
            })
        }),
        load_kobo_metadata_record: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                device_auth::load_kobo_metadata_record(database_file.as_path(), &book_id)
                    .await
                    .map(|value| value.map(map_test_kobo_metadata_record))
            })
        }),
        load_kobo_sync_page: Arc::new(
            |database_file,
             user,
             user_id,
             current_api_key_id,
             ongoing_sync_point_id,
             last_successful_sync_point_id,
             limit| {
                Box::pin(async move {
                    kobo_sync::load_kobo_sync_page(
                        database_file.as_path(),
                        &user,
                        &user_id,
                        current_api_key_id.as_deref(),
                        ongoing_sync_point_id.as_deref(),
                        last_successful_sync_point_id.as_deref(),
                        limit,
                    )
                    .await
                })
            },
        ),
        load_koreader_book_target: Arc::new(|database_file, book_hash| {
            Box::pin(async move {
                device_auth::load_koreader_book_target(database_file.as_path(), &book_hash)
                    .await
                    .map(|value| value.map(map_test_koreader_book_target))
                    .map_err(map_test_koreader_lookup_error)
            })
        }),
        load_read_progress: Arc::new(|database_file, book_id, user_id| {
            Box::pin(async move {
                device_auth::load_read_progress(database_file.as_path(), &book_id, &user_id)
                    .await
                    .map(|value| value.map(map_test_persisted_read_progress_record))
            })
        }),
        load_thumbnail_by_id: Arc::new(|database_file, thumbnail_id| {
            Box::pin(async move {
                device_auth::load_thumbnail_by_id(database_file.as_path(), &thumbnail_id).await
            })
        }),
        persist_read_progress_with_locator: Arc::new(
            |database_file,
             book_id,
             user_id,
             page,
             completed,
             device_id,
             device_name,
             timestamp,
             locator| {
                Box::pin(async move {
                    device_auth::persist_read_progress_with_locator(
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
                })
            },
        ),
        persisted_book_exists: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                device_auth::persisted_book_exists(database_file.as_path(), &book_id).await
            })
        }),
        proxy_kobo_store_library_sync: Arc::new(|forwarded_headers, query, raw_sync_token| {
            Box::pin(async move {
                kobo_sync::proxy_kobo_store_library_sync(
                    &forwarded_headers,
                    query.as_deref(),
                    &raw_sync_token,
                )
                .await
            })
        }),
        remove_sync_point: Arc::new(|database_file, sync_point_id| {
            Box::pin(async move {
                kobo_sync::remove_sync_point(database_file.as_path(), &sync_point_id).await
            })
        }),
        open_auth_pool: Arc::new(|database_file| {
            Box::pin(
                async move { infrastructure_auth::open_auth_pool(database_file.as_path()).await },
            )
        }),
    }
}

fn map_test_persisted_book_media_file(
    record: device_auth::PersistedBookMediaFile,
) -> PersistedBookMediaFile {
    PersistedBookMediaFile {
        file_name: record.file_name,
        media_type: record.media_type,
        file_path: record.file_path,
    }
}

fn map_test_persisted_read_progress_record(
    record: device_auth::PersistedReadProgressRecord,
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

fn map_test_koreader_book_target(record: device_auth::KoreaderBookTarget) -> KoreaderBookTarget {
    KoreaderBookTarget {
        id: record.id,
        page_count: record.page_count,
        media_type: record.media_type,
    }
}

fn map_test_kobo_metadata_record(record: device_auth::KoboMetadataRecord) -> KoboMetadataRecord {
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

fn map_test_koreader_lookup_error(
    error: device_auth::KoreaderBookLookupError,
) -> KoreaderBookLookupError {
    match error {
        device_auth::KoreaderBookLookupError::Persistence => KoreaderBookLookupError::Persistence,
        device_auth::KoreaderBookLookupError::Conflict => KoreaderBookLookupError::Conflict,
    }
}
