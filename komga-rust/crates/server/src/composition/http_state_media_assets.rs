use super::*;

struct ComposedMediaImportService {
    inner: komga_application::media_assets::MediaImportService<
        infrastructure_filesystem::FilesystemImportPort,
    >,
}

impl RuntimeMediaImportService for ComposedMediaImportService {
    fn enqueue_books(
        &self,
        payload: komga_application::media_assets::BooksImportPayload,
        next_task_id: &mut dyn FnMut() -> String,
    ) -> Result<Vec<komga_application::task_processing::TaskQueueRecord>, String> {
        self.inner.enqueue_books(payload, next_task_id)
    }

    fn process_queued_books_payload<'a>(
        &'a self,
        task_payload: &'a str,
    ) -> futures_util::future::BoxFuture<
        'a,
        Result<Vec<komga_application::task_processing::TaskQueueRecord>, String>,
    > {
        Box::pin(async move { self.inner.process_queued_books_payload(task_payload).await })
    }

    fn process_queued_book_payload<'a>(
        &'a self,
        task_payload: &'a str,
    ) -> futures_util::future::BoxFuture<
        'a,
        Result<Vec<komga_application::task_processing::TaskQueueRecord>, String>,
    > {
        Box::pin(async move { self.inner.process_queued_book_payload(task_payload).await })
    }
}

struct ComposedBookMetadataService {
    inner: komga_application::media_assets::BookMetadataService<
        infrastructure_metadata::SqliteBookMetadataPort,
    >,
}

impl RuntimeBookMetadataService for ComposedBookMetadataService {
    fn update_book_metadata<'a>(
        &'a self,
        book_id: &'a str,
        patch: &'a komga_application::media_assets::BookMetadataPatch,
    ) -> futures_util::future::BoxFuture<'a, Result<Option<Option<String>>, String>> {
        Box::pin(async move { self.inner.update_book_metadata(book_id, patch).await })
    }

    fn batch_update_book_metadata<'a>(
        &'a self,
        updates: Vec<(String, komga_application::media_assets::BookMetadataPatch)>,
    ) -> futures_util::future::BoxFuture<'a, Result<Vec<String>, String>> {
        Box::pin(async move { self.inner.batch_update_book_metadata(updates).await })
    }
}

pub(super) fn compose_media_assets_runtime_access_backend() -> MediaAssetsRuntimeAccessBackend {
    MediaAssetsRuntimeAccessBackend {
        media_import_service: Arc::new(|database_file| {
            Box::new(ComposedMediaImportService {
                inner: komga_application::media_assets::MediaImportService::new(
                    infrastructure_filesystem::FilesystemImportPort::new(database_file),
                ),
            })
        }),
        book_metadata_service: Arc::new(|database_file| {
            Box::new(ComposedBookMetadataService {
                inner: komga_application::media_assets::BookMetadataService::new(
                    infrastructure_metadata::SqliteBookMetadataPort::new(database_file),
                ),
            })
        }),
        persist_book_page_hashes_with_media_content: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::persist_book_page_hashes_from_media_content(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        decode_epub_positions: Arc::new(|blob| {
            infrastructure_filesystem::decode_epub_positions_blob(blob.as_slice())
        }),
        load_epub_archive_positions: Arc::new(|media| {
            infrastructure_filesystem::load_epub_archive_positions(&media)
        }),
        read_media_file_bytes: Arc::new(|path| std::fs::read(path).ok()),
        read_media_file_size: Arc::new(|path| {
            std::fs::metadata(path)
                .ok()
                .and_then(|meta| i64::try_from(meta.len()).ok())
        }),
        load_persisted_book_media: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_book_media(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        load_persisted_book_media_files: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_book_media_files(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        book_media_is_ready_status: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::book_media_is_ready_status(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        load_persisted_series_thumbnail_media: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_series_thumbnail_media(
                    database_file.as_path(),
                    &series_id,
                )
                .await
            })
        }),
        load_persisted_book_pages: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_book_pages(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        load_persisted_book_page_row: Arc::new(|database_file, book_id, page_number| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_book_page_row(
                    database_file.as_path(),
                    &book_id,
                    page_number,
                )
                .await
            })
        }),
        resolve_book_page_bytes: Arc::new(|media, page, page_number| {
            infrastructure_filesystem::resolve_book_page_bytes(&media, &page, page_number)
        }),
        load_archive_page_row: Arc::new(|media, page_number| {
            infrastructure_filesystem::load_archive_page_row(&media, page_number)
        }),
        load_archive_page_rows: Arc::new(|media| {
            infrastructure_filesystem::load_archive_page_rows(&media)
        }),
        load_pdf_page_row: Arc::new(|media, page_number| {
            infrastructure_filesystem::load_pdf_page_row(&media, page_number)
        }),
        load_generated_pdf_page_rows: Arc::new(|media| {
            infrastructure_filesystem::load_generated_pdf_page_rows(&media)
        }),
        read_pdf_page_as_single_page_pdf: Arc::new(|media, page_number| {
            infrastructure_filesystem::read_pdf_page_as_single_page_pdf(&media, page_number)
        }),
        detect_pdf_page_count: Arc::new(|media| {
            infrastructure_filesystem::detect_pdf_page_count(&media)
        }),
        load_persisted_epub_extension_blob: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_epub_extension_blob(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        load_series_book_ids: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_series_book_ids(database_file.as_path(), &series_id)
                    .await
            })
        }),
        refresh_series_read_progress_row: Arc::new(|database_file, series_id, user_id| {
            Box::pin(async move {
                infrastructure_filesystem::refresh_series_read_progress_row(
                    database_file.as_path(),
                    &series_id,
                    &user_id,
                )
                .await
            })
        }),
        delete_series_read_progress_row: Arc::new(|database_file, series_id, user_id| {
            Box::pin(async move {
                infrastructure_filesystem::delete_series_read_progress_row(
                    database_file.as_path(),
                    &series_id,
                    &user_id,
                )
                .await
            })
        }),
        load_series_tachiyomi_progress: Arc::new(|database_file, series_id, user_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_series_tachiyomi_progress(
                    database_file.as_path(),
                    &series_id,
                    &user_id,
                )
                .await
            })
        }),
        load_book_progression: Arc::new(|database_file, book_id, user_id| {
            Box::pin(async move {
                infrastructure_metadata::load_book_progression(
                    database_file.as_path(),
                    &book_id,
                    &user_id,
                )
                .await
            })
        }),
        persist_read_progress: Arc::new(
            |database_file, book_id, user_id, page, completed, locator| {
                Box::pin(async move {
                    infrastructure_metadata::persist_read_progress(
                        database_file.as_path(),
                        &book_id,
                        &user_id,
                        page,
                        completed,
                        locator,
                    )
                    .await
                })
            },
        ),
        delete_persisted_read_progress: Arc::new(|database_file, book_id, user_id| {
            Box::pin(async move {
                infrastructure_metadata::delete_persisted_read_progress(
                    database_file.as_path(),
                    &book_id,
                    &user_id,
                )
                .await
            })
        }),
        readlist_tachiyomi_counters: Arc::new(|database_file, readlist_id, user_id| {
            Box::pin(async move {
                infrastructure_metadata::readlist_tachiyomi_counters(
                    database_file.as_path(),
                    &readlist_id,
                    &user_id,
                )
                .await
            })
        }),
        persist_readlist_tachiyomi_progress: Arc::new(
            |database_file, readlist_id, user_id, last_book_read| {
                Box::pin(async move {
                    infrastructure_metadata::persist_readlist_tachiyomi_progress(
                        database_file.as_path(),
                        &readlist_id,
                        &user_id,
                        last_book_read,
                    )
                    .await
                })
            },
        ),
        load_selected_book_thumbnail: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_metadata::load_selected_book_thumbnail(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        load_book_thumbnail_by_id: Arc::new(|database_file, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::load_book_thumbnail_by_id(
                    database_file.as_path(),
                    &thumbnail_id,
                )
                .await
            })
        }),
        load_persisted_book_thumbnails: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_metadata::load_persisted_book_thumbnails(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        insert_book_thumbnail: Arc::new(
            |database_file, book_id, thumbnail, media_type, width, height, selected| {
                Box::pin(async move {
                    infrastructure_metadata::insert_book_thumbnail(
                        database_file.as_path(),
                        &book_id,
                        thumbnail.as_slice(),
                        &media_type,
                        width,
                        height,
                        selected,
                    )
                    .await
                })
            },
        ),
        select_book_thumbnail: Arc::new(|database_file, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::select_book_thumbnail(
                    database_file.as_path(),
                    &thumbnail_id,
                )
                .await
            })
        }),
        delete_book_thumbnail: Arc::new(|database_file, book_id, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::delete_book_thumbnail(
                    database_file.as_path(),
                    &book_id,
                    &thumbnail_id,
                )
                .await
            })
        }),
        load_persisted_readlist_thumbnails: Arc::new(|database_file, readlist_id| {
            Box::pin(async move {
                infrastructure_metadata::load_persisted_readlist_thumbnails(
                    database_file.as_path(),
                    &readlist_id,
                )
                .await
            })
        }),
        insert_readlist_thumbnail: Arc::new(
            |database_file, readlist_id, thumbnail, media_type, width, height, selected| {
                Box::pin(async move {
                    infrastructure_metadata::insert_readlist_thumbnail(
                        database_file.as_path(),
                        &readlist_id,
                        thumbnail.as_slice(),
                        &media_type,
                        width,
                        height,
                        selected,
                    )
                    .await
                })
            },
        ),
        select_readlist_thumbnail: Arc::new(|database_file, readlist_id, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::select_readlist_thumbnail(
                    database_file.as_path(),
                    &readlist_id,
                    &thumbnail_id,
                )
                .await
            })
        }),
        delete_readlist_thumbnail: Arc::new(|database_file, readlist_id, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::delete_readlist_thumbnail(
                    database_file.as_path(),
                    &readlist_id,
                    &thumbnail_id,
                )
                .await
            })
        }),
        load_persisted_collection_thumbnails: Arc::new(|database_file, collection_id| {
            Box::pin(async move {
                infrastructure_metadata::load_persisted_collection_thumbnails(
                    database_file.as_path(),
                    &collection_id,
                )
                .await
            })
        }),
        insert_collection_thumbnail: Arc::new(
            |database_file, collection_id, thumbnail, media_type, width, height, selected| {
                Box::pin(async move {
                    infrastructure_metadata::insert_collection_thumbnail(
                        database_file.as_path(),
                        &collection_id,
                        thumbnail.as_slice(),
                        &media_type,
                        width,
                        height,
                        selected,
                    )
                    .await
                })
            },
        ),
        select_collection_thumbnail: Arc::new(|database_file, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::select_collection_thumbnail(
                    database_file.as_path(),
                    &thumbnail_id,
                )
                .await
            })
        }),
        delete_collection_thumbnail: Arc::new(|database_file, collection_id, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::delete_collection_thumbnail(
                    database_file.as_path(),
                    &collection_id,
                    &thumbnail_id,
                )
                .await
            })
        }),
        load_selected_series_thumbnail: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_metadata::load_selected_series_thumbnail(
                    database_file.as_path(),
                    &series_id,
                )
                .await
            })
        }),
        load_persisted_series_thumbnails: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_metadata::load_persisted_series_thumbnails(
                    database_file.as_path(),
                    &series_id,
                )
                .await
            })
        }),
        load_series_thumbnail_by_id: Arc::new(|database_file, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::load_series_thumbnail_by_id(
                    database_file.as_path(),
                    &thumbnail_id,
                )
                .await
            })
        }),
        insert_series_thumbnail: Arc::new(
            |database_file, series_id, thumbnail, media_type, width, height, selected| {
                Box::pin(async move {
                    infrastructure_metadata::insert_series_thumbnail(
                        database_file.as_path(),
                        &series_id,
                        thumbnail.as_slice(),
                        &media_type,
                        width,
                        height,
                        selected,
                    )
                    .await
                })
            },
        ),
        select_series_thumbnail: Arc::new(|database_file, series_id, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::select_series_thumbnail(
                    database_file.as_path(),
                    &series_id,
                    &thumbnail_id,
                )
                .await
            })
        }),
        delete_series_thumbnail: Arc::new(|database_file, series_id, thumbnail_id| {
            Box::pin(async move {
                infrastructure_metadata::delete_series_thumbnail(
                    database_file.as_path(),
                    &series_id,
                    &thumbnail_id,
                )
                .await
            })
        }),
        load_persisted_readlist_name: Arc::new(|database_file, readlist_id| {
            Box::pin(async move {
                infrastructure_metadata::load_persisted_readlist_name(
                    database_file.as_path(),
                    &readlist_id,
                )
                .await
            })
        }),
        load_book_restrictions: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_book_restrictions(database_file.as_path(), &book_id)
                    .await
            })
        }),
        load_readlist_archive_entries: Arc::new(|database_file, readlist_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_readlist_archive_entries(
                    database_file.as_path(),
                    &readlist_id,
                )
                .await
            })
        }),
        load_series_archive_entries: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_series_archive_entries(
                    database_file.as_path(),
                    &series_id,
                )
                .await
            })
        }),
        is_font_resource: Arc::new(|resource_name| {
            infrastructure_filesystem::is_font_resource(&resource_name)
        }),
        read_epub_resource_bytes: Arc::new(|epub_path, resource_name| {
            infrastructure_filesystem::read_epub_resource_bytes(epub_path.as_path(), &resource_name)
        }),
        load_persisted_manifest_book: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_persisted_manifest_book(
                    database_file.as_path(),
                    &book_id,
                )
                .await
            })
        }),
        persisted_book_exists: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_filesystem::persisted_book_exists(database_file.as_path(), &book_id)
                    .await
            })
        }),
        persisted_book_ids: Arc::new(|database_file| {
            Box::pin(async move {
                infrastructure_filesystem::persisted_book_ids(database_file.as_path()).await
            })
        }),
        persisted_series_exists: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_filesystem::persisted_series_exists(
                    database_file.as_path(),
                    &series_id,
                )
                .await
            })
        }),
        persisted_readlist_exists: Arc::new(|database_file, readlist_id| {
            Box::pin(async move {
                infrastructure_metadata::persisted_readlist_exists(
                    database_file.as_path(),
                    &readlist_id,
                )
                .await
            })
        }),
        persisted_collection_exists: Arc::new(|database_file, collection_id| {
            Box::pin(async move {
                infrastructure_metadata::persisted_collection_exists(
                    database_file.as_path(),
                    &collection_id,
                )
                .await
            })
        }),
        load_series_book_number_sorts: Arc::new(|database_file, series_id| {
            Box::pin(async move {
                infrastructure_filesystem::load_series_book_number_sorts(
                    database_file.as_path(),
                    &series_id,
                )
                .await
            })
        }),
        load_book_page_count: Arc::new(|database_file, book_id| {
            Box::pin(async move {
                infrastructure_metadata::load_book_page_count(database_file.as_path(), &book_id)
                    .await
            })
        }),
        persist_book_progression: Arc::new(
            |database_file,
             book_id,
             user_id,
             page,
             use_locator_position_for_page,
             modified,
             device_id,
             device_name,
             locator| {
                Box::pin(async move {
                    infrastructure_metadata::persist_book_progression(
                        database_file.as_path(),
                        &book_id,
                        &user_id,
                        page,
                        use_locator_position_for_page,
                        modified,
                        device_id,
                        device_name,
                        locator,
                    )
                    .await
                })
            },
        ),
    }
}
