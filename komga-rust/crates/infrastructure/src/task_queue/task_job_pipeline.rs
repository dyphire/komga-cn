use std::collections::BTreeSet;

use crate::search::index_lifecycle::SearchEntityType;
use komga_application::task_processing::{
    ScanOneLibrary, TaskExecutionOutcome, TaskKind, TaskProcessingError, TaskQueueRecord,
};
use serde_json::Value;

use super::{
    JobRuntime,
    task_job_dispatch::{TaskJobCommand, TaskJobDispatcher},
};

pub(super) struct TaskJobPipeline<'a> {
    dispatcher: TaskJobDispatcher<'a>,
}

impl<'a> TaskJobPipeline<'a> {
    pub(super) fn new(runtime: JobRuntime<'a>) -> Self {
        Self {
            dispatcher: TaskJobDispatcher::new(runtime),
        }
    }

    pub(super) async fn execute(
        &self,
        task: &TaskQueueRecord,
    ) -> Result<TaskExecutionOutcome, TaskProcessingError> {
        let command = resolve_task_job(task)?;

        self.dispatcher.execute(command).await
    }
}

fn resolve_task_job(record: &TaskQueueRecord) -> Result<TaskJobCommand<'_>, TaskProcessingError> {
    let kind = TaskKind::parse(&record.simple_type)
        .map_err(|_| TaskProcessingError::unsupported_task(&record.simple_type))?;
    let target = super::task_identity::task_target(record);
    let job = match kind {
        TaskKind::ScanLibrary => TaskJobCommand::ScanLibrary(scan_library_request(record, target)?),
        TaskKind::HashBookPages => TaskJobCommand::HashBookPages {
            book_id: required_target(target, "HashBookPages task must include a book id")?,
        },
        TaskKind::HashBook => TaskJobCommand::HashBook {
            book_id: required_target(target, "HashBook task must include a book id")?,
            koreader: false,
        },
        TaskKind::HashBookKoreader => TaskJobCommand::HashBook {
            book_id: required_target(target, "HashBookKoreader task must include a book id")?,
            koreader: true,
        },
        TaskKind::FindBooksWithMissingPageHash => TaskJobCommand::FindBooksWithMissingPageHash {
            library_id: required_target(
                target,
                "FindBooksWithMissingPageHash task must include a library id",
            )?,
            priority: record.priority,
        },
        TaskKind::FindDuplicatePagesToDelete => TaskJobCommand::FindDuplicatePagesToDelete {
            library_id: required_target(
                target,
                "FindDuplicatePagesToDelete task must include a library id",
            )?,
            priority: record.priority,
        },
        TaskKind::RemoveHashedPages => {
            let book_id = required_target(target, "RemoveHashedPages task must include a book id")?;
            TaskJobCommand::RemoveHashedPages {
                book_id,
                pages: remove_hashed_pages_payload(record, book_id)?,
                priority: record.priority,
            }
        }
        TaskKind::AnalyzeBook => TaskJobCommand::AnalyzeBook {
            book_id: required_target(target, "AnalyzeBook task must include a book id")?,
            priority: record.priority,
        },
        TaskKind::RebuildIndex => TaskJobCommand::RebuildIndex {
            entity_types: parse_rebuild_index_entities(record.payload.as_deref())?,
        },
        TaskKind::UpgradeIndex => TaskJobCommand::UpgradeIndex,
        TaskKind::FindBookThumbnailsToRegenerate => {
            TaskJobCommand::FindBookThumbnailsToRegenerate {
                for_bigger_result_only: parse_for_bigger_result_only(record.payload.as_deref()),
                priority: record.priority,
            }
        }
        TaskKind::RefreshBookMetadata => TaskJobCommand::RefreshBookMetadata {
            book_id: required_target(target, "RefreshBookMetadata task must include a book id")?,
            capabilities: refresh_book_metadata_capabilities(record),
            priority: record.priority,
        },
        TaskKind::RefreshSeriesMetadata => TaskJobCommand::RefreshSeriesMetadata {
            series_id: required_target(
                target,
                "RefreshSeriesMetadata task must include a series id",
            )?,
            priority: record.priority,
        },
        TaskKind::AggregateSeriesMetadata => TaskJobCommand::AggregateSeriesMetadata {
            series_id: required_target(
                target,
                "AggregateSeriesMetadata task must include a series id",
            )?,
        },
        TaskKind::RefreshBookLocalArtwork => TaskJobCommand::RefreshBookLocalArtwork {
            book_id: required_target(
                target,
                "RefreshBookLocalArtwork task must include a book id",
            )?,
        },
        TaskKind::GenerateBookThumbnail => TaskJobCommand::GenerateBookThumbnail {
            book_id: required_target(target, "GenerateBookThumbnail task must include a book id")?,
        },
        TaskKind::RefreshSeriesLocalArtwork => TaskJobCommand::RefreshSeriesLocalArtwork {
            series_id: required_target(
                target,
                "RefreshSeriesLocalArtwork task must include a series id",
            )?,
        },
        TaskKind::EmptyTrash => TaskJobCommand::EmptyTrash {
            library_id: required_target(target, "EmptyTrash task must include a library id")?,
        },
        TaskKind::DeleteBook => TaskJobCommand::DeleteBook {
            book_id: required_target(target, "DeleteBook task must include a book id")?,
        },
        TaskKind::DeleteSeries => TaskJobCommand::DeleteSeries {
            series_id: required_target(target, "DeleteSeries task must include a series id")?,
        },
        TaskKind::RepairExtension => TaskJobCommand::RepairExtension {
            book_id: required_target(target, "RepairExtension task must include a book id")?,
        },
        TaskKind::FindBooksToConvert => TaskJobCommand::FindBooksToConvert {
            library_id: required_target(
                target,
                "FindBooksToConvert task must include a library id",
            )?,
            priority: record.priority,
        },
        TaskKind::ConvertBook => TaskJobCommand::ConvertBook {
            book_id: required_target(target, "ConvertBook task must include a book id")?,
        },
        TaskKind::ImportBook => TaskJobCommand::ImportBook {
            payload: import_book_payload(record)?,
            priority: record.priority,
        },
    };

    Ok(job)
}

fn required_target<'a>(
    target: Option<&'a str>,
    message: &str,
) -> Result<&'a str, TaskProcessingError> {
    target.ok_or_else(|| TaskProcessingError::invalid_task(message))
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct ScanTaskPayloadFields {
    library_id: Option<String>,
    deep_scan: Option<bool>,
}

fn scan_library_request(
    task: &TaskQueueRecord,
    task_target: Option<&str>,
) -> Result<ScanOneLibrary, TaskProcessingError> {
    let payload = task.payload.as_deref().and_then(scan_task_payload_fields);
    let library_id = payload
        .as_ref()
        .and_then(|fields| fields.library_id.clone())
        .or_else(|| task_target.map(scan_task_legacy_target_library_id));
    let Some(library_id) = library_id else {
        return Err(TaskProcessingError::invalid_task(
            "ScanLibrary task must include a library id",
        ));
    };

    let deep_scan = payload
        .and_then(|fields| fields.deep_scan)
        .or_else(|| task_target.and_then(scan_task_legacy_target_deep_scan))
        .unwrap_or(false);

    Ok(ScanOneLibrary::new(library_id, deep_scan))
}

fn scan_task_payload_fields(payload: &str) -> Option<ScanTaskPayloadFields> {
    let payload = serde_json::from_str::<serde_json::Value>(payload).ok()?;
    Some(ScanTaskPayloadFields {
        library_id: payload.get("libraryId")?.as_str().map(str::to_string),
        deep_scan: payload
            .get("scanDeep")
            .or_else(|| payload.get("deep"))
            .and_then(|value| value.as_bool()),
    })
}

fn scan_task_legacy_target_library_id(task_target: &str) -> String {
    task_target
        .split_once("_DEEP_")
        .map(|(id, _)| id)
        .unwrap_or(task_target)
        .to_string()
}

fn scan_task_legacy_target_deep_scan(task_target: &str) -> Option<bool> {
    task_target
        .rsplit_once("_DEEP_")
        .and_then(|(_, deep_scan)| deep_scan.parse::<bool>().ok())
}

fn remove_hashed_pages_payload(
    task: &TaskQueueRecord,
    book_id: &str,
) -> Result<Vec<super::HashedPageToDelete>, TaskProcessingError> {
    let Some(payload) = task.payload.as_deref() else {
        return Err(TaskProcessingError::invalid_task(
            "RemoveHashedPages task requires serialized payload",
        ));
    };
    let parsed =
        serde_json::from_str::<super::RemoveHashedPagesPayload>(payload).map_err(|error| {
            TaskProcessingError::runtime(format!(
                "failed to parse RemoveHashedPages payload: {error}",
            ))
        })?;
    if parsed.book_id != book_id {
        return Err(TaskProcessingError::invalid_task(
            "RemoveHashedPages payload book id must match task id",
        ));
    }
    if parsed.unique_id != task.id {
        return Err(TaskProcessingError::invalid_task(
            "RemoveHashedPages payload unique id must match task id",
        ));
    }

    Ok(parsed.pages)
}

fn parse_rebuild_index_entities(
    payload: Option<&str>,
) -> Result<Option<Vec<SearchEntityType>>, TaskProcessingError> {
    let Some(payload) = payload else {
        return Ok(None);
    };
    let payload = serde_json::from_str::<Value>(payload).map_err(|error| {
        TaskProcessingError::runtime(format!("RebuildIndex payload must be valid JSON: {error}"))
    })?;
    let Some(entities) = payload.get("entities") else {
        return Ok(None);
    };
    if entities.is_null() {
        return Ok(None);
    }
    let entity_values = entities.as_array().ok_or_else(|| {
        TaskProcessingError::invalid_task("RebuildIndex payload field 'entities' must be an array")
    })?;

    let mut parsed = Vec::new();
    for entity in entity_values {
        let entity_type = parse_rebuild_index_entity(entity).ok_or_else(|| {
            TaskProcessingError::runtime(format!(
                "RebuildIndex payload contains unsupported entity selector: {entity}"
            ))
        })?;
        if !parsed.contains(&entity_type) {
            parsed.push(entity_type);
        }
    }

    Ok(Some(parsed))
}

fn parse_rebuild_index_entity(value: &Value) -> Option<SearchEntityType> {
    let raw = match value {
        Value::String(value) => Some(value.as_str()),
        Value::Object(value) => value.get("type").and_then(Value::as_str),
        _ => None,
    }?;

    match raw.trim().to_ascii_lowercase().as_str() {
        "book" => Some(SearchEntityType::Book),
        "series" => Some(SearchEntityType::Series),
        "collection" => Some(SearchEntityType::Collection),
        "readlist" => Some(SearchEntityType::ReadList),
        _ => None,
    }
}

fn parse_for_bigger_result_only(payload: Option<&str>) -> bool {
    payload
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        .and_then(|payload| {
            payload
                .get("for_bigger_result_only")
                .or_else(|| payload.get("forBiggerResultOnly"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

fn refresh_book_metadata_capabilities(task: &TaskQueueRecord) -> BTreeSet<String> {
    task.payload
        .as_deref()
        .and_then(|payload| serde_json::from_str::<Value>(payload).ok())
        .and_then(|payload| payload.get("capabilities").cloned())
        .and_then(|capabilities| capabilities.as_array().cloned())
        .map(|capabilities| {
            capabilities
                .into_iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect::<BTreeSet<_>>()
        })
        .filter(|capabilities| !capabilities.is_empty())
        .unwrap_or_else(default_refresh_book_metadata_capabilities)
}

fn default_refresh_book_metadata_capabilities() -> BTreeSet<String> {
    [
        "TITLE",
        "SUMMARY",
        "NUMBER",
        "NUMBER_SORT",
        "RELEASE_DATE",
        "AUTHORS",
        "TAGS",
        "ISBN",
        "READ_LISTS",
        "THUMBNAILS",
        "LINKS",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn import_book_payload(task: &TaskQueueRecord) -> Result<String, TaskProcessingError> {
    task.payload.clone().ok_or_else(|| {
        TaskProcessingError::invalid_task("ImportBook task requires serialized payload")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn task_job_pipeline_reports_unsupported_task_types_at_the_command_parsing_boundary() {
        let fixture =
            super::super::test_support::RuntimeTestFixture::new("task-job-pipeline-unsupported");
        let runtime = fixture.runtime_context(true, true).await;
        let task =
            TaskQueueRecord::new("UnknownTask_target-1", 0, None).with_simple_type("UnknownTask");

        let error = TaskJobPipeline::new(runtime.job())
            .execute(&task)
            .await
            .expect_err("unknown task should fail at the task command parsing boundary");

        assert_eq!(error.message, "unsupported runtime task type: UnknownTask");
        fixture.cleanup().await;
    }

    #[test]
    fn scan_library_request_prefers_payload_over_legacy_target() {
        let task = TaskQueueRecord::new("ScanLibrary_missing-library_DEEP_true", 900, None)
            .with_simple_type("ScanLibrary")
            .with_payload(
                r#"{"libraryId":"library-1","scanDeep":false,"priority":900,"groupId":null,"uniqueId":"ScanLibrary_missing-library_DEEP_true"}"#,
            );

        let request = scan_library_request(&task, Some("missing-library_DEEP_true"))
            .expect("scan request should resolve from task payload");

        assert_eq!(request, ScanOneLibrary::new("library-1", false));
    }

    #[test]
    fn scan_library_request_uses_legacy_target_when_payload_is_absent() {
        let task = TaskQueueRecord::new("ScanLibrary_library-1_DEEP_true", 900, None)
            .with_simple_type("ScanLibrary");

        let request = scan_library_request(&task, Some("library-1_DEEP_true"))
            .expect("scan request should resolve from legacy task target");

        assert_eq!(request, ScanOneLibrary::new("library-1", true));
    }

    #[test]
    fn rebuild_index_payload_accepts_kotlin_entity_names_at_command_parsing_boundary() {
        let task = TaskQueueRecord::new("RebuildIndex", 10, None)
            .with_simple_type("RebuildIndex")
            .with_payload(r#"{"entities":["Collection","Series"]}"#);

        let job = resolve_task_job(&task).expect("rebuild index task should resolve");

        match job {
            TaskJobCommand::RebuildIndex {
                entity_types: Some(entity_types),
            } => {
                assert_eq!(
                    entity_types,
                    vec![SearchEntityType::Collection, SearchEntityType::Series],
                );
            }
            _ => panic!("rebuild index should resolve to typed entity selectors"),
        }
    }

    #[test]
    fn thumbnail_finder_payload_accepts_kotlin_camel_case_flag_at_command_parsing_boundary() {
        let task = TaskQueueRecord::new("FindBookThumbnailsToRegenerate", 6, None)
            .with_simple_type("FindBookThumbnailsToRegenerate")
            .with_payload(r#"{"forBiggerResultOnly":true}"#);

        let job = resolve_task_job(&task).expect("thumbnail finder task should resolve");

        match job {
            TaskJobCommand::FindBookThumbnailsToRegenerate {
                for_bigger_result_only,
                priority,
            } => {
                assert!(for_bigger_result_only);
                assert_eq!(priority, 6);
            }
            _ => panic!("thumbnail finder should resolve to typed regeneration selector"),
        }
    }
}
