use std::collections::BTreeSet;

use super::{
    FindBookThumbnailsToRegeneratePayload, HashedPageToDeletePayload, ImportBookPayload,
    RebuildIndexEntity, RebuildIndexPayload, RefreshBookMetadataPayload, RemoveHashedPagesPayload,
    ScanLibraryPayload, ScanOneLibrary, TaskKind, TaskProcessingError, TaskQueueRecord,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeTaskRequest {
    ScanLibrary(ScanOneLibrary),
    HashBookPages {
        book_id: String,
    },
    HashBook {
        book_id: String,
        koreader: bool,
    },
    FindBooksWithMissingPageHash {
        library_id: String,
        priority: i32,
    },
    FindDuplicatePagesToDelete {
        library_id: String,
        priority: i32,
    },
    RemoveHashedPages {
        book_id: String,
        pages: Vec<HashedPageToDeletePayload>,
        priority: i32,
    },
    AnalyzeBook {
        book_id: String,
        priority: i32,
    },
    RebuildIndex {
        entities: Option<Vec<RebuildIndexEntity>>,
    },
    UpgradeIndex,
    FindBookThumbnailsToRegenerate {
        for_bigger_result_only: bool,
        priority: i32,
    },
    RefreshBookMetadata {
        book_id: String,
        capabilities: BTreeSet<String>,
        priority: i32,
    },
    RefreshSeriesMetadata {
        series_id: String,
        priority: i32,
    },
    AggregateSeriesMetadata {
        series_id: String,
    },
    RefreshBookLocalArtwork {
        book_id: String,
    },
    GenerateBookThumbnail {
        book_id: String,
    },
    RefreshSeriesLocalArtwork {
        series_id: String,
    },
    EmptyTrash {
        library_id: String,
    },
    DeleteBook {
        book_id: String,
    },
    DeleteSeries {
        series_id: String,
    },
    RepairExtension {
        book_id: String,
    },
    FindBooksToConvert {
        library_id: String,
        priority: i32,
    },
    ConvertBook {
        book_id: String,
    },
    ImportBook {
        payload: ImportBookPayload,
        priority: i32,
    },
}

impl RuntimeTaskRequest {
    pub fn from_queue_record(record: &TaskQueueRecord) -> Result<Self, TaskProcessingError> {
        let kind = TaskKind::parse(&record.simple_type)
            .map_err(|_| TaskProcessingError::unsupported_task(&record.simple_type))?;
        let target = record.target();

        match kind {
            TaskKind::ScanLibrary => scan_library_request(record),
            TaskKind::HashBookPages => Ok(Self::HashBookPages {
                book_id: required_target(target, "HashBookPages task must include a book id")?,
            }),
            TaskKind::HashBook => Ok(Self::HashBook {
                book_id: required_target(target, "HashBook task must include a book id")?,
                koreader: false,
            }),
            TaskKind::HashBookKoreader => Ok(Self::HashBook {
                book_id: required_target(target, "HashBookKoreader task must include a book id")?,
                koreader: true,
            }),
            TaskKind::FindBooksWithMissingPageHash => Ok(Self::FindBooksWithMissingPageHash {
                library_id: required_target(
                    target,
                    "FindBooksWithMissingPageHash task must include a library id",
                )?,
                priority: record.priority,
            }),
            TaskKind::FindDuplicatePagesToDelete => Ok(Self::FindDuplicatePagesToDelete {
                library_id: required_target(
                    target,
                    "FindDuplicatePagesToDelete task must include a library id",
                )?,
                priority: record.priority,
            }),
            TaskKind::RemoveHashedPages => remove_hashed_pages_request(record, target),
            TaskKind::AnalyzeBook => Ok(Self::AnalyzeBook {
                book_id: required_target(target, "AnalyzeBook task must include a book id")?,
                priority: record.priority,
            }),
            TaskKind::RebuildIndex => rebuild_index_request(record),
            TaskKind::UpgradeIndex => Ok(Self::UpgradeIndex),
            TaskKind::FindBookThumbnailsToRegenerate => thumbnail_regeneration_request(record),
            TaskKind::RefreshBookMetadata => refresh_book_metadata_request(record, target),
            TaskKind::RefreshSeriesMetadata => Ok(Self::RefreshSeriesMetadata {
                series_id: required_target(
                    target,
                    "RefreshSeriesMetadata task must include a series id",
                )?,
                priority: record.priority,
            }),
            TaskKind::AggregateSeriesMetadata => Ok(Self::AggregateSeriesMetadata {
                series_id: required_target(
                    target,
                    "AggregateSeriesMetadata task must include a series id",
                )?,
            }),
            TaskKind::RefreshBookLocalArtwork => Ok(Self::RefreshBookLocalArtwork {
                book_id: required_target(
                    target,
                    "RefreshBookLocalArtwork task must include a book id",
                )?,
            }),
            TaskKind::GenerateBookThumbnail => Ok(Self::GenerateBookThumbnail {
                book_id: required_target(
                    target,
                    "GenerateBookThumbnail task must include a book id",
                )?,
            }),
            TaskKind::RefreshSeriesLocalArtwork => Ok(Self::RefreshSeriesLocalArtwork {
                series_id: required_target(
                    target,
                    "RefreshSeriesLocalArtwork task must include a series id",
                )?,
            }),
            TaskKind::EmptyTrash => Ok(Self::EmptyTrash {
                library_id: required_target(target, "EmptyTrash task must include a library id")?,
            }),
            TaskKind::DeleteBook => Ok(Self::DeleteBook {
                book_id: required_target(target, "DeleteBook task must include a book id")?,
            }),
            TaskKind::DeleteSeries => Ok(Self::DeleteSeries {
                series_id: required_target(target, "DeleteSeries task must include a series id")?,
            }),
            TaskKind::RepairExtension => Ok(Self::RepairExtension {
                book_id: required_target(target, "RepairExtension task must include a book id")?,
            }),
            TaskKind::FindBooksToConvert => Ok(Self::FindBooksToConvert {
                library_id: required_target(
                    target,
                    "FindBooksToConvert task must include a library id",
                )?,
                priority: record.priority,
            }),
            TaskKind::ConvertBook => Ok(Self::ConvertBook {
                book_id: required_target(target, "ConvertBook task must include a book id")?,
            }),
            TaskKind::ImportBook => Ok(Self::ImportBook {
                payload: ImportBookPayload::from_task_record(record)?,
                priority: record.priority,
            }),
        }
    }
}

fn required_target(target: Option<&str>, message: &str) -> Result<String, TaskProcessingError> {
    target
        .map(str::to_string)
        .ok_or_else(|| TaskProcessingError::invalid_task(message))
}

fn scan_library_request(
    record: &TaskQueueRecord,
) -> Result<RuntimeTaskRequest, TaskProcessingError> {
    let payload = ScanLibraryPayload::from_task_record(record)?;

    Ok(RuntimeTaskRequest::ScanLibrary(ScanOneLibrary::new(
        payload.library_id,
        payload.deep_scan,
    )))
}

fn remove_hashed_pages_request(
    record: &TaskQueueRecord,
    target: Option<&str>,
) -> Result<RuntimeTaskRequest, TaskProcessingError> {
    let book_id = required_target(target, "RemoveHashedPages task must include a book id")?;
    let payload = RemoveHashedPagesPayload::from_task_record(record, &book_id)?;

    Ok(RuntimeTaskRequest::RemoveHashedPages {
        book_id,
        pages: payload.pages,
        priority: record.priority,
    })
}

fn rebuild_index_request(
    record: &TaskQueueRecord,
) -> Result<RuntimeTaskRequest, TaskProcessingError> {
    let payload = RebuildIndexPayload::from_task_record(record)?;

    Ok(RuntimeTaskRequest::RebuildIndex {
        entities: payload.entities,
    })
}

fn thumbnail_regeneration_request(
    record: &TaskQueueRecord,
) -> Result<RuntimeTaskRequest, TaskProcessingError> {
    let payload = FindBookThumbnailsToRegeneratePayload::from_task_record(record)?;

    Ok(RuntimeTaskRequest::FindBookThumbnailsToRegenerate {
        for_bigger_result_only: payload.for_bigger_result_only,
        priority: record.priority,
    })
}

fn refresh_book_metadata_request(
    record: &TaskQueueRecord,
    target: Option<&str>,
) -> Result<RuntimeTaskRequest, TaskProcessingError> {
    let book_id = required_target(target, "RefreshBookMetadata task must include a book id")?;
    let payload = RefreshBookMetadataPayload::from_task_record(record, &book_id)?;

    Ok(RuntimeTaskRequest::RefreshBookMetadata {
        book_id,
        capabilities: payload.capabilities_for_execution(),
        priority: record.priority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_processing::{
        HashedPageToDeletePayload, ImportBookCopyMode, RemoveHashedPagesPayload, TaskRequest,
    };

    #[test]
    fn unsupported_task_fails_at_application_task_request_boundary() {
        let task =
            TaskQueueRecord::new("UnknownTask_target-1", 0, None).with_simple_type("UnknownTask");

        let error = RuntimeTaskRequest::from_queue_record(&task)
            .expect_err("unknown task should fail before infrastructure dispatch");

        assert_eq!(error.message, "unsupported runtime task type: UnknownTask");
    }

    #[test]
    fn rebuild_index_payload_accepts_kotlin_entity_names() {
        let task = TaskQueueRecord::new("RebuildIndex", 10, None)
            .with_simple_type("RebuildIndex")
            .with_payload(r#"{"entities":["Collection","Series"]}"#);

        let request =
            RuntimeTaskRequest::from_queue_record(&task).expect("rebuild index task should parse");

        assert_eq!(
            request,
            RuntimeTaskRequest::RebuildIndex {
                entities: Some(vec![
                    RebuildIndexEntity::Collection,
                    RebuildIndexEntity::Series
                ]),
            },
        );
    }

    #[test]
    fn scan_library_payload_parses_into_runtime_scan_request() {
        let record = TaskRequest::with_payload(
            TaskKind::ScanLibrary,
            ScanLibraryPayload::new("library-1", true),
        )
        .into_queue_record();

        let request = RuntimeTaskRequest::from_queue_record(&record)
            .expect("ScanLibrary runtime task request should parse");

        assert_eq!(
            request,
            RuntimeTaskRequest::ScanLibrary(ScanOneLibrary::new("library-1", true)),
        );
    }

    #[test]
    fn payload_backed_task_records_parse_at_application_boundary() {
        let record = TaskRequest::with_payload(
            TaskKind::RemoveHashedPages,
            RemoveHashedPagesPayload::new(
                "book-1",
                vec![HashedPageToDeletePayload {
                    file_hash: "hash-1".to_string(),
                    file_size: 123,
                    file_name: "page-1.jpg".to_string(),
                    media_type: "image/jpeg".to_string(),
                    page_number: 1,
                }],
            ),
        )
        .priority(12)
        .into_queue_record();

        let request = RuntimeTaskRequest::from_queue_record(&record)
            .expect("runtime task request should parse");

        match request {
            RuntimeTaskRequest::RemoveHashedPages {
                book_id,
                pages,
                priority,
            } => {
                assert_eq!(book_id, "book-1");
                assert_eq!(priority, 12);
                assert_eq!(pages.len(), 1);
                assert_eq!(pages[0].file_hash, "hash-1");
            }
            _ => panic!("RemoveHashedPages should parse through the application boundary"),
        }
    }

    #[test]
    fn import_book_payload_parses_at_application_boundary() {
        let record = TaskRequest::with_payload(
            TaskKind::ImportBook,
            ImportBookPayload::new(
                "/tmp/book-a.cbz",
                "series-1",
                ImportBookCopyMode::Hardlink,
                Some("dest-a".to_string()),
                Some("book-1".to_string()),
            ),
        )
        .priority(100)
        .group("series-1")
        .into_queue_record_with_id("series-1_/tmp/book-a.cbz");

        let request = RuntimeTaskRequest::from_queue_record(&record)
            .expect("ImportBook runtime task request should parse");

        match request {
            RuntimeTaskRequest::ImportBook { payload, priority } => {
                assert_eq!(priority, 100);
                assert_eq!(payload.source_file, "/tmp/book-a.cbz");
                assert_eq!(payload.series_id, "series-1");
                assert_eq!(payload.copy_mode, ImportBookCopyMode::Hardlink);
                assert_eq!(payload.destination_name.as_deref(), Some("dest-a"));
                assert_eq!(payload.upgrade_book_id.as_deref(), Some("book-1"));
            }
            _ => panic!("ImportBook should parse through the application boundary"),
        }
    }

    #[test]
    fn thumbnail_finder_payload_accepts_kotlin_camel_case_flag() {
        let task = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 6, None)
            .with_simple_type("FindBookThumbnailsToRegenerate")
            .with_payload(r#"{"forBiggerResultOnly":true}"#);

        let request = RuntimeTaskRequest::from_queue_record(&task)
            .expect("thumbnail finder task should parse");

        assert_eq!(
            request,
            RuntimeTaskRequest::FindBookThumbnailsToRegenerate {
                for_bigger_result_only: true,
                priority: 6,
            },
        );
    }
}
