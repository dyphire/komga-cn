use super::TaskQueueRecord;
use komga_application::task_processing::{
    DefaultTaskProtocolCatalog, PlannedTaskKind, TaskProtocolCatalog, TaskSchedule,
};
use serde_json::json;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RuntimeFollowUpTask {
    AnalyzeBook {
        book_id: String,
        series_id: String,
        priority: i32,
    },
    RefreshBookMetadata {
        book_id: String,
        series_id: Option<String>,
        priority: i32,
    },
    RefreshSeriesMetadata {
        series_id: String,
        priority: i32,
    },
    AggregateSeriesMetadata {
        series_id: String,
        priority: i32,
    },
    RefreshBookLocalArtwork {
        book_id: String,
        priority: i32,
    },
    RefreshSeriesLocalArtwork {
        series_id: String,
        priority: i32,
    },
    GenerateBookThumbnail {
        book_id: String,
        priority: i32,
    },
    HashBook {
        book_id: String,
        priority: i32,
    },
    HashBookKoreader {
        book_id: String,
        priority: i32,
    },
    HashBookPages {
        book_id: String,
        priority: i32,
    },
    FindBooksWithMissingPageHash {
        library_id: String,
    },
    FindDuplicatePagesToDelete {
        library_id: String,
        priority: i32,
    },
    RepairExtension {
        book_id: String,
        series_id: String,
        priority: i32,
    },
    RemoveHashedPages {
        book_id: String,
        priority: i32,
        payload: String,
    },
    FindBooksToConvert {
        library_id: String,
        priority: i32,
    },
    ConvertBook {
        book_id: String,
        series_id: String,
        priority: i32,
    },
}

pub(super) fn runtime_follow_up_task(task: RuntimeFollowUpTask) -> TaskQueueRecord {
    DefaultTaskProtocolCatalog
        .plan_task_from_follow_up(task)
        .into_queue_record()
}

pub(super) fn runtime_startup_task(simple_type: &'static str) -> TaskQueueRecord {
    DefaultTaskProtocolCatalog
        .plan_task_from_runtime_simple_type(simple_type, TaskSchedule::Startup, 1_000, None, None)
        .map(komga_application::task_processing::PlannedTask::into_queue_record)
        .unwrap_or_else(|| TaskQueueRecord::new(simple_type.to_string(), 1_000, None))
}

trait RuntimeTaskProtocolExt {
    fn plan_task_from_follow_up(
        &self,
        task: RuntimeFollowUpTask,
    ) -> komga_application::task_processing::PlannedTask;

    fn plan_task_from_runtime_simple_type(
        &self,
        simple_type: &str,
        schedule: TaskSchedule,
        priority: i32,
        group: Option<String>,
        payload: Option<String>,
    ) -> Option<komga_application::task_processing::PlannedTask>;
}

impl<T> RuntimeTaskProtocolExt for T
where
    T: TaskProtocolCatalog,
{
    fn plan_task_from_follow_up(
        &self,
        task: RuntimeFollowUpTask,
    ) -> komga_application::task_processing::PlannedTask {
        match task {
            RuntimeFollowUpTask::AnalyzeBook {
                book_id,
                series_id,
                priority,
            } => self.plan_task(
                PlannedTaskKind::AnalyzeBook,
                TaskSchedule::Background,
                format!("ANALYZE_BOOK_{book_id}"),
                priority,
                Some(series_id),
                None,
            ),
            RuntimeFollowUpTask::RefreshBookMetadata {
                book_id,
                series_id,
                priority,
            } => self.plan_task(
                PlannedTaskKind::RefreshBookMetadata,
                TaskSchedule::Background,
                format!("REFRESH_BOOK_METADATA_{book_id}"),
                priority,
                series_id,
                None,
            ),
            RuntimeFollowUpTask::RefreshSeriesMetadata {
                series_id,
                priority,
            } => {
                let task_id = format!("REFRESH_SERIES_METADATA_{series_id}");
                self.plan_task(
                    PlannedTaskKind::RefreshSeriesMetadata,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    Some(series_id.clone()),
                    Some(book_task_payload(
                        "seriesId",
                        &series_id,
                        priority,
                        Some(series_id.as_str()),
                        &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::AggregateSeriesMetadata {
                series_id,
                priority,
            } => {
                let task_id = format!("AGGREGATE_SERIES_METADATA_{series_id}");
                self.plan_task(
                    PlannedTaskKind::AggregateSeriesMetadata,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    Some(series_id.clone()),
                    Some(book_task_payload(
                        "seriesId",
                        &series_id,
                        priority,
                        Some(series_id.as_str()),
                        &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::RefreshBookLocalArtwork { book_id, priority } => self.plan_task(
                PlannedTaskKind::RefreshBookLocalArtwork,
                TaskSchedule::Background,
                format!("REFRESH_BOOK_LOCAL_ARTWORK_{book_id}"),
                priority,
                None,
                None,
            ),
            RuntimeFollowUpTask::RefreshSeriesLocalArtwork {
                series_id,
                priority,
            } => self.plan_task(
                PlannedTaskKind::RefreshSeriesLocalArtwork,
                TaskSchedule::Background,
                format!("REFRESH_SERIES_LOCAL_ARTWORK_{series_id}"),
                priority,
                None,
                None,
            ),
            RuntimeFollowUpTask::GenerateBookThumbnail { book_id, priority } => self.plan_task(
                PlannedTaskKind::GenerateBookThumbnail,
                TaskSchedule::Background,
                format!("GENERATE_BOOK_THUMBNAIL_{book_id}"),
                priority,
                None,
                None,
            ),
            RuntimeFollowUpTask::HashBook { book_id, priority } => {
                let task_id = format!("HASH_BOOK_{book_id}");
                self.plan_task(
                    PlannedTaskKind::HashBook,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    None,
                    Some(book_task_payload(
                        "bookId", &book_id, priority, None, &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::HashBookKoreader { book_id, priority } => {
                let task_id = format!("HASH_BOOK_KOREADER_{book_id}");
                self.plan_task(
                    PlannedTaskKind::HashBookKoreader,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    None,
                    Some(book_task_payload(
                        "bookId", &book_id, priority, None, &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::HashBookPages { book_id, priority } => {
                let task_id = format!("HASH_BOOK_PAGES_{book_id}");
                self.plan_task(
                    PlannedTaskKind::HashBookPages,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    None,
                    Some(book_task_payload(
                        "bookId", &book_id, priority, None, &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::FindBooksWithMissingPageHash { library_id } => {
                let task_id = format!("FIND_BOOKS_WITH_MISSING_PAGE_HASH_{library_id}");
                self.plan_task(
                    PlannedTaskKind::FindBooksWithMissingPageHash,
                    TaskSchedule::Background,
                    task_id.clone(),
                    0,
                    None,
                    Some(book_task_payload(
                        "libraryId",
                        &library_id,
                        0,
                        None,
                        &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::FindDuplicatePagesToDelete {
                library_id,
                priority,
            } => self.plan_task(
                PlannedTaskKind::FindDuplicatePagesToDelete,
                TaskSchedule::Background,
                format!("FIND_DUPLICATE_PAGES_TO_DELETE_{library_id}"),
                priority,
                None,
                None,
            ),
            RuntimeFollowUpTask::RepairExtension {
                book_id,
                series_id,
                priority,
            } => {
                let task_id = format!("REPAIR_EXTENSION_{book_id}");
                self.plan_task(
                    PlannedTaskKind::RepairExtension,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    Some(series_id.clone()),
                    Some(book_task_payload(
                        "bookId",
                        &book_id,
                        priority,
                        Some(series_id.as_str()),
                        &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::RemoveHashedPages {
                book_id,
                priority,
                payload,
            } => self.plan_task(
                PlannedTaskKind::RemoveHashedPages,
                TaskSchedule::Background,
                format!("REMOVE_HASHED_PAGES_{book_id}"),
                priority,
                None,
                Some(payload),
            ),
            RuntimeFollowUpTask::FindBooksToConvert {
                library_id,
                priority,
            } => {
                let task_id = format!("FIND_BOOKS_TO_CONVERT_{library_id}");
                self.plan_task(
                    PlannedTaskKind::FindBooksToConvert,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    None,
                    Some(book_task_payload(
                        "libraryId",
                        &library_id,
                        priority,
                        None,
                        &task_id,
                    )),
                )
            }
            RuntimeFollowUpTask::ConvertBook {
                book_id,
                series_id,
                priority,
            } => {
                let task_id = format!("CONVERT_BOOK_{book_id}");
                self.plan_task(
                    PlannedTaskKind::ConvertBook,
                    TaskSchedule::Background,
                    task_id.clone(),
                    priority,
                    Some(series_id.clone()),
                    Some(book_task_payload(
                        "bookId",
                        &book_id,
                        priority,
                        Some(series_id.as_str()),
                        &task_id,
                    )),
                )
            }
        }
    }

    fn plan_task_from_runtime_simple_type(
        &self,
        simple_type: &str,
        schedule: TaskSchedule,
        priority: i32,
        group: Option<String>,
        payload: Option<String>,
    ) -> Option<komga_application::task_processing::PlannedTask> {
        let kind = self.known_kind_from_runtime_simple_type(simple_type)?;
        Some(self.plan_task(
            kind,
            schedule,
            simple_type.to_string(),
            priority,
            group,
            payload,
        ))
    }
}

fn book_task_payload(
    target_key: &str,
    target_value: &str,
    priority: i32,
    group_id: Option<&str>,
    task_id: &str,
) -> String {
    json!({
        target_key: target_value,
        "priority": priority,
        "groupId": group_id,
        "uniqueId": task_id,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_follow_up_helper_preserves_kotlin_compatible_shapes() {
        let refresh = runtime_follow_up_task(RuntimeFollowUpTask::RefreshBookMetadata {
            book_id: "book-1".to_string(),
            series_id: Some("series-1".to_string()),
            priority: 12,
        });
        assert_eq!(refresh.id, "REFRESH_BOOK_METADATA_book-1");
        assert_eq!(refresh.simple_type, "REFRESH_BOOK_METADATA");
        assert_eq!(refresh.group.as_deref(), Some("series-1"));

        let hash_pages = runtime_follow_up_task(RuntimeFollowUpTask::HashBookPages {
            book_id: "book-2".to_string(),
            priority: 3,
        });
        assert_eq!(hash_pages.id, "HASH_BOOK_PAGES_book-2");
        assert_eq!(hash_pages.simple_type, "HASH_BOOK_PAGES");
        assert_eq!(
            hash_pages.payload.as_deref(),
            Some(
                r#"{"bookId":"book-2","groupId":null,"priority":3,"uniqueId":"HASH_BOOK_PAGES_book-2"}"#,
            ),
        );

        let convert = runtime_follow_up_task(RuntimeFollowUpTask::ConvertBook {
            book_id: "book-3".to_string(),
            series_id: "series-3".to_string(),
            priority: 4,
        });
        assert_eq!(convert.id, "CONVERT_BOOK_book-3");
        assert_eq!(convert.simple_type, "CONVERT_BOOK");
        assert_eq!(convert.group.as_deref(), Some("series-3"));
        assert_eq!(
            convert.payload.as_deref(),
            Some(
                r#"{"bookId":"book-3","groupId":"series-3","priority":4,"uniqueId":"CONVERT_BOOK_book-3"}"#,
            ),
        );
    }

    #[test]
    fn runtime_follow_up_helper_builds_runtime_only_follow_ups() {
        let aggregate = runtime_follow_up_task(RuntimeFollowUpTask::AggregateSeriesMetadata {
            series_id: "series-1".to_string(),
            priority: 7,
        });
        assert_eq!(aggregate.id, "AGGREGATE_SERIES_METADATA_series-1");
        assert_eq!(aggregate.simple_type, "AGGREGATE_SERIES_METADATA");
        assert_eq!(aggregate.group.as_deref(), Some("series-1"));
        assert_eq!(
            aggregate.payload.as_deref(),
            Some(
                r#"{"groupId":"series-1","priority":7,"seriesId":"series-1","uniqueId":"AGGREGATE_SERIES_METADATA_series-1"}"#,
            ),
        );

        let remove = runtime_follow_up_task(RuntimeFollowUpTask::RemoveHashedPages {
            book_id: "book-5".to_string(),
            priority: 11,
            payload: r#"{"bookId":"book-5"}"#.to_string(),
        });
        assert_eq!(remove.id, "REMOVE_HASHED_PAGES_book-5");
        assert_eq!(remove.simple_type, "REMOVE_HASHED_PAGES");
        assert_eq!(remove.payload.as_deref(), Some(r#"{"bookId":"book-5"}"#));

        let page_hash = runtime_follow_up_task(RuntimeFollowUpTask::FindBooksWithMissingPageHash {
            library_id: "library-9".to_string(),
        });
        assert_eq!(page_hash.id, "FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-9");
        assert_eq!(page_hash.simple_type, "FIND_BOOKS_WITH_MISSING_PAGE_HASH");
        assert_eq!(
            page_hash.payload.as_deref(),
            Some(
                r#"{"groupId":null,"libraryId":"library-9","priority":0,"uniqueId":"FIND_BOOKS_WITH_MISSING_PAGE_HASH_library-9"}"#,
            ),
        );

        let convert_scan = runtime_follow_up_task(RuntimeFollowUpTask::FindBooksToConvert {
            library_id: "library-1".to_string(),
            priority: 0,
        });
        assert_eq!(convert_scan.id, "FIND_BOOKS_TO_CONVERT_library-1");
        assert_eq!(convert_scan.simple_type, "FIND_BOOKS_TO_CONVERT");
        assert_eq!(
            convert_scan.payload.as_deref(),
            Some(
                r#"{"groupId":null,"libraryId":"library-1","priority":0,"uniqueId":"FIND_BOOKS_TO_CONVERT_library-1"}"#,
            ),
        );
    }

    #[test]
    fn runtime_startup_helper_uses_catalog_descriptor_for_known_tasks() {
        let startup = runtime_startup_task("REBUILD_INDEX");

        assert_eq!(startup.id, "REBUILD_INDEX");
        assert_eq!(startup.simple_type, "REBUILD_INDEX");
        assert_eq!(startup.priority, 1_000);
        assert!(startup.payload.is_none());
    }
}
