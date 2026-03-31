use super::*;

pub(super) fn compose_server_settings_store(database_file: &Path) -> InterfacesServerSettingsStore {
    let store = Arc::new(komga_infrastructure::ServerSettingsStore::new(
        database_file.to_path_buf(),
    ));
    InterfacesServerSettingsStore::new(
        Arc::new({
            let store = store.clone();
            move || {
                let store = store.clone();
                Box::pin(async move {
                    store
                        .load_map()
                        .await
                        .map(|value| value.into_iter().collect())
                        .map_err(|error| error.to_string())
                })
            }
        }),
        Arc::new(move |changes| {
            let store = store.clone();
            Box::pin(async move {
                store
                    .apply_changes(changes.as_slice())
                    .await
                    .map_err(|error| error.to_string())
            })
        }),
    )
}

pub(super) fn compose_operational_runtime_access_backend() -> OperationalRuntimeAccessBackend {
    OperationalRuntimeAccessBackend {
        load_task_execution_values: Arc::new(|tasks_db_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_task_execution_values(
                    tasks_db_file.as_path(),
                )
                .await
            })
        }),
        load_libraries_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_libraries_count(database_file.as_path())
                    .await
            })
        }),
        load_series_grouped_by_library: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_series_grouped_by_library(
                    database_file.as_path(),
                )
                .await
            })
        }),
        load_books_grouped_by_library: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_books_grouped_by_library(
                    database_file.as_path(),
                )
                .await
            })
        }),
        load_books_filesize_grouped_by_library: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_books_filesize_grouped_by_library(
                    database_file.as_path(),
                )
                .await
            })
        }),
        load_sidecars_grouped_by_library: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_sidecars_grouped_by_library(
                    database_file.as_path(),
                )
                .await
            })
        }),
        load_collections_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_collections_count(database_file.as_path())
                    .await
            })
        }),
        load_readlists_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_readlists_count(database_file.as_path())
                    .await
            })
        }),
        load_task_failure_count: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_operational_metrics::load_task_failure_count(database_file.as_path())
                    .await
            })
        }),
        load_sse_snapshot: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                let snapshot = komga_infrastructure::sqlite::read_models::load_sse_snapshot(
                    database_file.as_path(),
                    &user_id,
                )
                .await;
                map_sse_snapshot(snapshot)
            })
        }),
    }
}

fn map_sse_snapshot(
    value: komga_infrastructure::sqlite::read_models::SseSnapshot,
) -> InterfacesSseSnapshot {
    InterfacesSseSnapshot {
        libraries: value
            .libraries
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesLibrarySnapshot {
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        series: value
            .series
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesSeriesSnapshot {
                        library_id: row.library_id,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        books: value
            .books
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesBookSnapshot {
                        series_id: row.series_id,
                        library_id: row.library_id,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        readlists: value
            .readlists
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesReadListSnapshot {
                        book_ids: row.book_ids,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        collections: value
            .collections
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesCollectionSnapshot {
                        series_ids: row.series_ids,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        thumbnails_book: value
            .thumbnails_book
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesThumbnailBookSnapshot {
                        series_id: row.series_id,
                        selected: row.selected,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        thumbnails_series: value
            .thumbnails_series
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesThumbnailSnapshot {
                        selected: row.selected,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        thumbnails_collection: value
            .thumbnails_collection
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesThumbnailSnapshot {
                        selected: row.selected,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        thumbnails_readlist: value
            .thumbnails_readlist
            .into_iter()
            .map(|(id, row)| {
                (
                    id,
                    InterfacesThumbnailSnapshot {
                        selected: row.selected,
                        last_modified: row.last_modified,
                    },
                )
            })
            .collect(),
        read_progress: value.read_progress,
        read_progress_series: value.read_progress_series,
    }
}

pub(super) fn compose_operational_settings_access_backend() -> OperationalSettingsAccessBackend {
    OperationalSettingsAccessBackend {
        load_announcement_read_ids: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                komga_infrastructure::announcements_access::load_announcement_read_ids(
                    database_file.as_path(),
                    &user_id,
                )
                .await
            })
        }),
        save_announcements_read: Arc::new(|database_file, user_id, ids| {
            Box::pin(async move {
                komga_infrastructure::announcements_access::save_announcements_read(
                    database_file.as_path(),
                    &user_id,
                    ids.as_slice(),
                )
                .await
            })
        }),
        load_claim_status: Arc::new(|database_file| {
            Box::pin(async move {
                komga_infrastructure::claims_access::load_claim_status(database_file.as_path())
                    .await
            })
        }),
        claim_initial_admin_user: Arc::new(|database_file, user_id, email, password_hash| {
            Box::pin(async move {
                komga_infrastructure::claims_access::claim_initial_admin_user(
                    database_file.as_path(),
                    &user_id,
                    &email,
                    &password_hash,
                )
                .await
                .map(|value| match value {
                    komga_infrastructure::claims_access::ClaimInitialAdminUserResult::Created(user) => {
                        InterfacesClaimInitialAdminUserResult::Created(
                            komga_application::identity_access::AuthUser {
                                id: user.id,
                                email: user.email,
                                password: String::new(),
                                roles: vec!["ADMIN".to_string()],
                                shared_all_libraries: true,
                                shared_library_ids: Vec::new(),
                                labels_allow: Vec::new(),
                                labels_exclude: Vec::new(),
                                age_restriction: None,
                            },
                        )
                    }
                    komga_infrastructure::claims_access::ClaimInitialAdminUserResult::AlreadyClaimed => {
                        InterfacesClaimInitialAdminUserResult::AlreadyClaimed
                    }
                })
            })
        }),
        load_client_settings_global: Arc::new(|database_file, allow_unauthorized_only| {
            Box::pin(async move {
                infrastructure_operational_settings::load_client_settings_global(
                    database_file.as_path(),
                    allow_unauthorized_only,
                )
                .await
            })
        }),
        load_client_settings_user: Arc::new(|database_file, user_id| {
            Box::pin(async move {
                infrastructure_operational_settings::load_client_settings_user(
                    database_file.as_path(),
                    &user_id,
                )
                .await
            })
        }),
        upsert_client_settings_global: Arc::new(|database_file, settings| {
            Box::pin(async move {
                infrastructure_operational_settings::upsert_client_settings_global(
                    database_file.as_path(),
                    settings.as_slice(),
                )
                .await
            })
        }),
        upsert_client_settings_user: Arc::new(|database_file, user_id, settings| {
            Box::pin(async move {
                infrastructure_operational_settings::upsert_client_settings_user(
                    database_file.as_path(),
                    &user_id,
                    settings.as_slice(),
                )
                .await
            })
        }),
        delete_client_settings_global: Arc::new(|database_file, keys| {
            Box::pin(async move {
                infrastructure_operational_settings::delete_client_settings_global(
                    database_file.as_path(),
                    keys.as_slice(),
                )
                .await
            })
        }),
        delete_client_settings_user: Arc::new(|database_file, user_id, keys| {
            Box::pin(async move {
                infrastructure_operational_settings::delete_client_settings_user(
                    database_file.as_path(),
                    &user_id,
                    keys.as_slice(),
                )
                .await
            })
        }),
        list_directory_entries: Arc::new(|path, directories_only| {
            infrastructure_filesystem::list_directory_entries(path.as_path(), directories_only)
        }),
        list_font_families: Arc::new(|path| {
            infrastructure_filesystem::list_font_families(path.as_path())
        }),
        load_font_family_css: Arc::new(|path, family| {
            infrastructure_filesystem::load_font_family_css(path.as_path(), &family)
        }),
        load_font_file: Arc::new(|path, family, file| {
            infrastructure_filesystem::load_font_file(path.as_path(), &family, &file)
        }),
        delete_syncpoints_by_user_and_key_id: Arc::new(|database_file, user_id, key_id| {
            Box::pin(async move {
                infrastructure_operational_settings::delete_syncpoints_by_user_and_key_id(
                    database_file.as_path(),
                    &user_id,
                    &key_id,
                )
                .await
            })
        }),
        load_history_page: Arc::new(|database_file, page, size| {
            Box::pin(async move {
                infrastructure_operational_settings::load_history_page(
                    database_file.as_path(),
                    page,
                    size,
                )
                .await
            })
        }),
        load_page_hash_matches_page: Arc::new(|database_file, page_hash, page, size| {
            Box::pin(async move {
                infrastructure_page_hashes::load_page_hash_matches_page(
                    database_file.as_path(),
                    &page_hash,
                    page,
                    size,
                )
                .await
            })
        }),
        load_page_hash_thumbnail: Arc::new(|database_file, page_hash| {
            Box::pin(async move {
                infrastructure_page_hashes::load_page_hash_thumbnail(
                    database_file.as_path(),
                    &page_hash,
                )
                .await
                .map(|value| {
                    value.map(|row| InterfacesPageHashThumbnail {
                        media_type: row.media_type,
                        bytes: row.bytes,
                    })
                })
            })
        }),
        load_page_hashes_page: Arc::new(|database_file, page, size| {
            Box::pin(async move {
                infrastructure_page_hashes::load_page_hashes_page(
                    database_file.as_path(),
                    page,
                    size,
                )
                .await
            })
        }),
        load_page_hashes_unknown_page: Arc::new(|database_file, page, size| {
            Box::pin(async move {
                infrastructure_page_hashes::load_page_hashes_unknown_page(
                    database_file.as_path(),
                    page,
                    size,
                )
                .await
            })
        }),
        upsert_page_hash: Arc::new(|database_file, hash, size, action| {
            Box::pin(async move {
                infrastructure_page_hashes::upsert_page_hash(
                    database_file.as_path(),
                    &hash,
                    size,
                    &action,
                )
                .await
            })
        }),
        delete_all_page_hash_matches: Arc::new(|database_file, hash| {
            Box::pin(async move {
                infrastructure_page_hashes::delete_all_page_hash_matches(
                    database_file.as_path(),
                    &hash,
                )
                .await
                .map(|_| ())
            })
        }),
        delete_page_hash_match: Arc::new(|database_file, hash, media_id, page_number| {
            Box::pin(async move {
                infrastructure_page_hashes::delete_page_hash_match(
                    database_file.as_path(),
                    &hash,
                    &media_id,
                    page_number,
                )
                .await
                .map(|_| ())
            })
        }),
        load_server_settings: Arc::new(|settings_store| {
            Box::pin(async move {
                let persisted = settings_store.load_map().await?;
                let remember_me_key = persisted
                    .get("REMEMBER_ME_KEY")
                    .and_then(|value| value.as_ref())
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(generated_remember_me_key);

                if !persisted.contains_key("REMEMBER_ME_KEY")
                    || persisted
                        .get("REMEMBER_ME_KEY")
                        .is_some_and(|value| value.as_deref().unwrap_or_default().trim().is_empty())
                {
                    settings_store
                        .apply_changes(&[(
                            "REMEMBER_ME_KEY".to_string(),
                            Some(remember_me_key.clone()),
                        )])
                        .await?;
                }

                Ok(InterfacesPersistedServerSettings {
                    delete_empty_collections: persisted
                        .get("DELETE_EMPTY_COLLECTIONS")
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")),
                    delete_empty_read_lists: persisted
                        .get("DELETE_EMPTY_READLISTS")
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")),
                    remember_me_key,
                    remember_me_duration_days: persisted
                        .get("REMEMBER_ME_DURATION")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .unwrap_or(365),
                    thumbnail_size: match persisted
                        .get("THUMBNAIL_SIZE")
                        .and_then(|value| value.as_deref())
                    {
                        Some("MEDIUM") => "MEDIUM",
                        Some("LARGE") => "LARGE",
                        Some("XLARGE") => "XLARGE",
                        _ => "DEFAULT",
                    },
                    task_pool_size: persisted
                        .get("TASK_POOL_SIZE")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u64>().ok())
                        .unwrap_or(1),
                    server_port: persisted
                        .get("SERVER_PORT")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u16>().ok()),
                    server_context_path: persisted
                        .get("SERVER_CONTEXT_PATH")
                        .and_then(|value| value.as_ref())
                        .cloned(),
                    kobo_proxy: persisted
                        .get("KOBO_PROXY")
                        .and_then(|value| value.as_deref())
                        .is_some_and(|value| value.trim().eq_ignore_ascii_case("true")),
                    kobo_port: persisted
                        .get("KOBO_PORT")
                        .and_then(|value| value.as_deref())
                        .and_then(|value| value.trim().parse::<u16>().ok()),
                })
            })
        }),
        apply_server_settings_changes: Arc::new(|settings_store, changes| {
            Box::pin(async move { settings_store.apply_changes(changes.as_slice()).await })
        }),
        analyze_transient_book: Arc::new(|path| {
            infrastructure_filesystem::analyze_transient_book(&path).map(|value| {
                InterfacesTransientBookAnalysis {
                    status: value.status,
                    media_type: value.media_type,
                    pages: value
                        .pages
                        .into_iter()
                        .map(|page| InterfacesTransientBookPage {
                            number: page.number,
                            file_name: page.file_name,
                            media_type: page.media_type,
                            width: page.width,
                            height: page.height,
                            size_bytes: page.size_bytes,
                        })
                        .collect(),
                    files: value.files,
                    comment: value.comment,
                    number: value.number,
                    series_id: value.series_id,
                }
            })
        }),
        infer_transient_series_and_number: Arc::new(|database_file, transient_name| {
            Box::pin(async move {
                infrastructure_filesystem::infer_transient_series_and_number(
                    database_file.as_path(),
                    &transient_name,
                )
                .await
            })
        }),
        list_transient_book_entries: Arc::new(|root| {
            infrastructure_filesystem::list_transient_book_entries(root.as_path())
        }),
        load_transient_book_file_metadata: Arc::new(|path| {
            infrastructure_filesystem::load_transient_book_file_metadata(&path).map(|value| {
                InterfacesTransientBookFileMetadata {
                    file_last_modified_epoch_seconds: value.file_last_modified_epoch_seconds,
                    size_bytes: value.size_bytes,
                }
            })
        }),
        load_transient_book_media: Arc::new(|path| {
            infrastructure_filesystem::load_transient_book_media(&path)
        }),
        transient_book_content_type: Arc::new(|path, media_type| {
            infrastructure_filesystem::transient_book_content_type(&path, &media_type)
        }),
        transient_book_exists: Arc::new(|path| {
            infrastructure_filesystem::transient_book_exists(&path)
        }),
        transient_book_media_type: Arc::new(|path| {
            infrastructure_filesystem::transient_book_media_type(&path)
        }),
        transient_book_page_content: Arc::new(|path, media_type, pages, page_number| {
            let pages = pages
                .into_iter()
                .map(|page| infrastructure_filesystem::TransientBookPage {
                    number: page.number,
                    file_name: page.file_name,
                    media_type: page.media_type,
                    width: page.width,
                    height: page.height,
                    size_bytes: page.size_bytes,
                })
                .collect::<Vec<_>>();
            infrastructure_filesystem::transient_book_page_content(
                &path,
                &media_type,
                pages.as_slice(),
                page_number,
            )
        }),
    }
}

fn generated_remember_me_key() -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time should be after epoch")
        .as_nanos();
    let sequence = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let raw = format!("{nanos:032x}{sequence:016x}");
    raw.chars().take(32).collect()
}
