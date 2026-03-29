use super::*;

pub(super) fn compose_runtime_identity_access_backend() -> RuntimeIdentityAccessBackend {
    RuntimeIdentityAccessBackend {
        auth_token_user: Arc::new(|headers| infrastructure_auth::auth_token_user(&headers)),
        session_token_for_user_with_namespace: Arc::new(|user, namespace| {
            infrastructure_auth::session_token_for_user_with_namespace(&user, &namespace)
        }),
        remember_me_token_for_user_with_namespace: Arc::new(|user, namespace| {
            infrastructure_auth::remember_me_token_for_user_with_namespace(&user, &namespace)
        }),
        configure_remember_me_store: Arc::new(|store_root| {
            infrastructure_auth::configure_remember_me_store(store_root.as_path())
        }),
        invalidate_user_sessions: Arc::new(|user_id| {
            infrastructure_auth::invalidate_user_sessions(&user_id)
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
            |database_file, user, source, api_key_id, api_key_comment| {
                Box::pin(async move {
                    infrastructure_auth::persisted_record_successful_authentication_activity(
                        database_file.as_path(),
                        &user,
                        &source,
                        api_key_id.as_deref(),
                        api_key_comment.as_deref(),
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
        configured_api_key: Arc::new(infrastructure_auth::configured_api_key),
        configured_api_key_comment: Arc::new(infrastructure_auth::configured_api_key_comment),
        configured_api_key_id: Arc::new(infrastructure_auth::configured_api_key_id),
        load_book_created_timestamp: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_auth::load_book_created_timestamp(database_file.as_path(), &book_id)
                    .await
            })
        }),
        load_book_last_epub_position_locator: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_auth::load_book_last_epub_position_locator(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        load_book_media_file: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_auth::load_book_media_file(database_file.as_path(), &book_id)
                    .await
                    .map(|value| value.map(map_persisted_book_media_file))
            })
        }),
        load_book_page_count: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_auth::load_book_page_count(database_file.as_path(), &book_id).await
            })
        }),
        load_kobo_metadata_record: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_auth::load_kobo_metadata_record(database_file.as_path(), &book_id)
                    .await
                    .map(|value| value.map(map_kobo_metadata_record))
            })
        }),
        load_kobo_sync_snapshot: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                infrastructure_auth::load_kobo_sync_snapshot(database_file.as_path(), &user_id)
                    .await
            })
        }),
        load_koreader_book_target: Arc::new(|database_file, book_hash| {
            Box::pin(async move {
                infrastructure_auth::load_koreader_book_target(database_file.as_path(), &book_hash)
                    .await
                    .map(|value| value.map(map_koreader_book_target))
                    .map_err(map_koreader_lookup_error)
            })
        }),
        load_read_progress: Arc::new(|database_file, book_id, user_id| {
            Box::pin(async move {
                infrastructure_auth::load_read_progress(database_file.as_path(), &book_id, &user_id)
                    .await
                    .map(|value| value.map(map_persisted_read_progress_record))
            })
        }),
        load_sync_point_marker: Arc::new(|database_file, sync_point_id, user_id| {
            Box::pin(async move {
                infrastructure_auth::load_sync_point_marker(
                    database_file.as_path(),
                    &sync_point_id,
                    &user_id,
                )
                .await
            })
        }),
        load_sync_point_state: Arc::new(|database_file, sync_point_id, user_id| {
            Box::pin(async move {
                infrastructure_auth::load_sync_point_state(
                    database_file.as_path(),
                    &sync_point_id,
                    &user_id,
                )
                .await
            })
        }),
        load_thumbnail_by_id: Arc::new(|database_file, thumbnail_id| {
            Box::pin(async move {
                infrastructure_auth::load_thumbnail_by_id(database_file.as_path(), &thumbnail_id)
                    .await
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
                    infrastructure_auth::persist_read_progress_with_locator(
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
                infrastructure_auth::persisted_book_exists(database_file.as_path(), &book_id).await
            })
        }),
        proxy_kobo_store_library_sync: Arc::new(|forwarded_headers, query, raw_sync_token| {
            Box::pin(async move {
                infrastructure_auth::proxy_kobo_store_library_sync(
                    &forwarded_headers,
                    query.as_deref(),
                    &raw_sync_token,
                )
                .await
            })
        }),
        remove_sync_point: Arc::new(|database_file, sync_point_id| {
            Box::pin(async move {
                infrastructure_auth::remove_sync_point(database_file.as_path(), &sync_point_id)
                    .await
            })
        }),
        save_sync_point: Arc::new(|database_file, sync_point_id, sync_point_state| {
            Box::pin(async move {
                infrastructure_auth::save_sync_point(
                    database_file.as_path(),
                    &sync_point_id,
                    &sync_point_state,
                )
                .await
            })
        }),
        create_auth_user: Arc::new(|database_file, input| {
            Box::pin(async move {
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
            })
        }),
        delete_auth_user: Arc::new(|database_file, target_user_id| {
            Box::pin(async move {
                infrastructure_runtime_identity::delete_auth_user(
                    database_file.as_path(),
                    &target_user_id,
                )
                .await
            })
        }),
        update_auth_user: Arc::new(|database_file, target_user_id, patch| {
            Box::pin(async move {
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
            })
        }),
        open_auth_pool: Arc::new(|database_file| {
            Box::pin(
                async move { infrastructure_auth::open_auth_pool(database_file.as_path()).await },
            )
        }),
    }
}

fn map_persisted_book_media_file(
    record: infrastructure_auth::PersistedBookMediaFile,
) -> InterfacesPersistedBookMediaFile {
    InterfacesPersistedBookMediaFile {
        file_name: record.file_name,
        media_type: record.media_type,
        file_path: record.file_path,
    }
}

fn map_persisted_read_progress_record(
    record: infrastructure_auth::PersistedReadProgressRecord,
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
    record: infrastructure_auth::KoreaderBookTarget,
) -> InterfacesKoreaderBookTarget {
    InterfacesKoreaderBookTarget {
        id: record.id,
        page_count: record.page_count,
    }
}

fn map_kobo_metadata_record(
    record: infrastructure_auth::KoboMetadataRecord,
) -> InterfacesKoboMetadataRecord {
    InterfacesKoboMetadataRecord {
        title: record.title,
        summary: record.summary,
        release_date: record.release_date,
        language: record.language,
        file_size: record.file_size,
        file_name: record.file_name,
    }
}

fn map_koreader_lookup_error(
    error: infrastructure_auth::KoreaderBookLookupError,
) -> InterfacesKoreaderBookLookupError {
    match error {
        infrastructure_auth::KoreaderBookLookupError::Persistence => {
            InterfacesKoreaderBookLookupError::Persistence
        }
        infrastructure_auth::KoreaderBookLookupError::Conflict => {
            InterfacesKoreaderBookLookupError::Conflict
        }
    }
}
