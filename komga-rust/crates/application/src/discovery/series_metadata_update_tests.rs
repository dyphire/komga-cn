use std::sync::{Arc, Mutex};

use komga_domain::discovery::SeriesStatus;

use super::{
    ExistingSeriesMetadataRecord, SeriesAlternateTitleRecord, SeriesEventEmitter,
    SeriesMetadataLinkRecord, SeriesMetadataPatch, SeriesMetadataUpdateError,
    SeriesMetadataUpdateRecord, SeriesMetadataUpdateResult, SeriesMetadataWritePort,
    SeriesMetadataWriter, SeriesReadingDirection,
};

struct RecordingSeriesMetadataPort {
    existing: Option<ExistingSeriesMetadataRecord>,
    library_id: Option<String>,
    persisted: Mutex<Vec<RecordedSeriesMetadataUpdate>>,
    synced: Mutex<Vec<String>>,
    steps: Arc<Mutex<Vec<&'static str>>>,
}

struct RecordingSeriesEventEmitter {
    emitted: Mutex<Vec<RecordedSeriesChangedEvent>>,
    steps: Arc<Mutex<Vec<&'static str>>>,
}

struct RecordedSeriesMetadataUpdate {
    series_id: String,
    update: SeriesMetadataUpdateRecord,
}

#[derive(Debug, PartialEq, Eq)]
struct RecordedSeriesChangedEvent {
    series_id: String,
    library_id: String,
}

impl SeriesEventEmitter for RecordingSeriesEventEmitter {
    fn emit_series_changed(&self, series_id: &str, library_id: &str) {
        self.steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .push("emit");
        self.emitted
            .lock()
            .expect("emitted event lock should not be poisoned")
            .push(RecordedSeriesChangedEvent {
                series_id: series_id.to_string(),
                library_id: library_id.to_string(),
            });
    }
}

#[async_trait::async_trait]
impl SeriesMetadataWritePort for RecordingSeriesMetadataPort {
    async fn load_series_library_id(&self, _series_id: &str) -> anyhow::Result<Option<String>> {
        self.steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .push("load-library");
        Ok(self.library_id.clone())
    }

    async fn load_existing_series_metadata(
        &self,
        _series_id: &str,
    ) -> anyhow::Result<Option<ExistingSeriesMetadataRecord>> {
        self.steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .push("load-existing");
        Ok(self.existing.clone())
    }

    async fn persist_series_metadata_update(
        &self,
        series_id: &str,
        update: SeriesMetadataUpdateRecord,
    ) -> anyhow::Result<bool> {
        self.steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .push("persist");
        self.persisted
            .lock()
            .expect("persisted update lock should not be poisoned")
            .push(RecordedSeriesMetadataUpdate {
                series_id: series_id.to_string(),
                update,
            });
        Ok(true)
    }

    async fn refresh_series_search_documents_after_metadata_update(
        &self,
        series_id: &str,
    ) -> anyhow::Result<()> {
        self.steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .push("sync");
        self.synced
            .lock()
            .expect("search sync lock should not be poisoned")
            .push(series_id.to_string());
        Ok(())
    }
}

#[tokio::test]
async fn series_metadata_writer_merges_patch_emits_event_and_syncs_search() {
    let steps = Arc::new(Mutex::new(Vec::new()));
    let port = RecordingSeriesMetadataPort {
        existing: Some(existing_metadata()),
        library_id: Some("library-1".to_string()),
        persisted: Mutex::new(Vec::new()),
        synced: Mutex::new(Vec::new()),
        steps: steps.clone(),
    };
    let event_emitter = RecordingSeriesEventEmitter {
        emitted: Mutex::new(Vec::new()),
        steps: steps.clone(),
    };
    let writer = SeriesMetadataWriter::new(&port, &event_emitter);

    let result = writer
        .update_series(
            "series-1",
            SeriesMetadataPatch {
                title: Some("Updated Title".to_string()),
                title_lock: Some(true),
                age_rating: Some(Some(16)),
                tags: Some(vec!["z".to_string(), "a".to_string()]),
                links: Some(vec![SeriesMetadataLinkRecord {
                    label: "Site".to_string(),
                    url: "https://example.org".to_string(),
                }]),
                ..SeriesMetadataPatch::default()
            },
        )
        .await
        .expect("series metadata update should succeed");

    assert_eq!(result, SeriesMetadataUpdateResult::Updated);
    let persisted = port
        .persisted
        .lock()
        .expect("persisted update lock should not be poisoned");
    assert_eq!(persisted.len(), 1);
    assert_eq!(persisted[0].series_id, "series-1");
    assert_eq!(persisted[0].update.title, "Updated Title");
    assert!(persisted[0].update.title_lock);
    assert_eq!(persisted[0].update.status, SeriesStatus::Ongoing);
    assert_eq!(persisted[0].update.age_rating, Some(16));
    assert_eq!(
        persisted[0].update.tags,
        vec!["z".to_string(), "a".to_string()]
    );
    assert_eq!(
        persisted[0].update.links,
        vec![SeriesMetadataLinkRecord {
            label: "Site".to_string(),
            url: "https://example.org".to_string(),
        }]
    );
    drop(persisted);
    assert_eq!(
        port.synced
            .lock()
            .expect("search sync lock should not be poisoned")
            .as_slice(),
        ["series-1"]
    );
    assert_eq!(
        event_emitter
            .emitted
            .lock()
            .expect("emitted event lock should not be poisoned")
            .as_slice(),
        [RecordedSeriesChangedEvent {
            series_id: "series-1".to_string(),
            library_id: "library-1".to_string(),
        }]
    );
    assert_eq!(
        steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .as_slice(),
        ["load-existing", "persist", "load-library", "emit", "sync"]
    );
}

#[tokio::test]
async fn series_metadata_writer_returns_not_found_without_side_effects() {
    let steps = Arc::new(Mutex::new(Vec::new()));
    let port = RecordingSeriesMetadataPort {
        existing: None,
        library_id: Some("library-1".to_string()),
        persisted: Mutex::new(Vec::new()),
        synced: Mutex::new(Vec::new()),
        steps: steps.clone(),
    };
    let event_emitter = RecordingSeriesEventEmitter {
        emitted: Mutex::new(Vec::new()),
        steps: steps.clone(),
    };
    let writer = SeriesMetadataWriter::new(&port, &event_emitter);

    let result = writer
        .update_series("missing-series", SeriesMetadataPatch::default())
        .await
        .expect("missing series metadata update should not fail");

    assert_eq!(result, SeriesMetadataUpdateResult::NotFound);
    assert!(
        port.persisted
            .lock()
            .expect("persisted update lock should not be poisoned")
            .is_empty()
    );
    assert!(
        port.synced
            .lock()
            .expect("search sync lock should not be poisoned")
            .is_empty()
    );
    assert!(
        event_emitter
            .emitted
            .lock()
            .expect("emitted event lock should not be poisoned")
            .is_empty()
    );
    assert_eq!(
        steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .as_slice(),
        ["load-existing"]
    );
}

#[tokio::test]
async fn series_metadata_writer_rejects_invalid_patch_before_missing_series_short_circuit() {
    let steps = Arc::new(Mutex::new(Vec::new()));
    let port = RecordingSeriesMetadataPort {
        existing: None,
        library_id: Some("library-1".to_string()),
        persisted: Mutex::new(Vec::new()),
        synced: Mutex::new(Vec::new()),
        steps: steps.clone(),
    };
    let event_emitter = RecordingSeriesEventEmitter {
        emitted: Mutex::new(Vec::new()),
        steps: steps.clone(),
    };
    let writer = SeriesMetadataWriter::new(&port, &event_emitter);

    let error = writer
        .update_series(
            "missing-series",
            SeriesMetadataPatch {
                title: Some(" ".to_string()),
                ..SeriesMetadataPatch::default()
            },
        )
        .await
        .expect_err("invalid patch should be rejected before missing-series handling");

    assert_eq!(
        error,
        SeriesMetadataUpdateError::Validation("title must not be blank".to_string()),
    );
    assert!(
        port.persisted
            .lock()
            .expect("persisted update lock should not be poisoned")
            .is_empty()
    );
    assert!(
        port.synced
            .lock()
            .expect("search sync lock should not be poisoned")
            .is_empty()
    );
    assert!(
        event_emitter
            .emitted
            .lock()
            .expect("emitted event lock should not be poisoned")
            .is_empty()
    );
    assert!(
        steps
            .lock()
            .expect("writer step lock should not be poisoned")
            .is_empty()
    );
}

#[tokio::test]
async fn series_metadata_writer_rejects_invalid_semantic_patch_values_without_side_effects() {
    let cases = [
        (
            SeriesMetadataPatch {
                title_sort: Some(" ".to_string()),
                ..SeriesMetadataPatch::default()
            },
            "titleSort must not be blank",
        ),
        (
            SeriesMetadataPatch {
                age_rating: Some(Some(i32::MAX as u32 + 1)),
                ..SeriesMetadataPatch::default()
            },
            "ageRating must be between 0 and 2147483647",
        ),
        (
            SeriesMetadataPatch {
                language: Some("en_US".to_string()),
                ..SeriesMetadataPatch::default()
            },
            "language must be blank or a valid BCP47 language tag",
        ),
        (
            SeriesMetadataPatch {
                total_book_count: Some(Some(0)),
                ..SeriesMetadataPatch::default()
            },
            "totalBookCount must be a positive integer",
        ),
        (
            SeriesMetadataPatch {
                total_book_count: Some(Some(i32::MAX as u32 + 1)),
                ..SeriesMetadataPatch::default()
            },
            "totalBookCount must be a positive integer",
        ),
        (
            SeriesMetadataPatch {
                links: Some(vec![SeriesMetadataLinkRecord {
                    label: " ".to_string(),
                    url: "https://example.org".to_string(),
                }]),
                ..SeriesMetadataPatch::default()
            },
            "links.label must not be blank",
        ),
        (
            SeriesMetadataPatch {
                links: Some(vec![SeriesMetadataLinkRecord {
                    label: "Site".to_string(),
                    url: "not-a-url".to_string(),
                }]),
                ..SeriesMetadataPatch::default()
            },
            "links.url must be a valid URL",
        ),
        (
            SeriesMetadataPatch {
                alternate_titles: Some(vec![SeriesAlternateTitleRecord {
                    label: " ".to_string(),
                    title: "Alt".to_string(),
                }]),
                ..SeriesMetadataPatch::default()
            },
            "alternateTitles.label must not be blank",
        ),
        (
            SeriesMetadataPatch {
                alternate_titles: Some(vec![SeriesAlternateTitleRecord {
                    label: "en".to_string(),
                    title: " ".to_string(),
                }]),
                ..SeriesMetadataPatch::default()
            },
            "alternateTitles.title must not be blank",
        ),
    ];

    for (patch, expected_error) in cases {
        let steps = Arc::new(Mutex::new(Vec::new()));
        let port = RecordingSeriesMetadataPort {
            existing: Some(existing_metadata()),
            library_id: Some("library-1".to_string()),
            persisted: Mutex::new(Vec::new()),
            synced: Mutex::new(Vec::new()),
            steps: steps.clone(),
        };
        let event_emitter = RecordingSeriesEventEmitter {
            emitted: Mutex::new(Vec::new()),
            steps: steps.clone(),
        };
        let writer = SeriesMetadataWriter::new(&port, &event_emitter);

        let error = writer
            .update_series("series-1", patch)
            .await
            .expect_err("invalid series metadata patch should be rejected by application");

        assert_eq!(
            error,
            SeriesMetadataUpdateError::Validation(expected_error.to_string()),
        );
        assert!(
            port.persisted
                .lock()
                .expect("persisted update lock should not be poisoned")
                .is_empty()
        );
        assert!(
            port.synced
                .lock()
                .expect("search sync lock should not be poisoned")
                .is_empty()
        );
        assert!(
            event_emitter
                .emitted
                .lock()
                .expect("emitted event lock should not be poisoned")
                .is_empty()
        );
        assert!(
            steps
                .lock()
                .expect("writer step lock should not be poisoned")
                .is_empty()
        );
    }
}

fn existing_metadata() -> ExistingSeriesMetadataRecord {
    ExistingSeriesMetadataRecord {
        status: SeriesStatus::Ongoing,
        status_lock: false,
        title: "Original Title".to_string(),
        title_lock: false,
        title_sort: "Original Title".to_string(),
        title_sort_lock: false,
        summary: "summary".to_string(),
        summary_lock: false,
        reading_direction: Some(SeriesReadingDirection::LeftToRight),
        reading_direction_lock: false,
        publisher: "publisher".to_string(),
        publisher_lock: false,
        age_rating: None,
        age_rating_lock: false,
        language: "en".to_string(),
        language_lock: false,
        genres: vec!["genre".to_string()],
        genres_lock: false,
        tags: vec!["tag".to_string()],
        tags_lock: false,
        total_book_count: Some(2),
        total_book_count_lock: false,
        sharing_labels: vec!["label".to_string()],
        sharing_labels_lock: false,
        links: vec![SeriesMetadataLinkRecord {
            label: "Old".to_string(),
            url: "https://old.example.org".to_string(),
        }],
        links_lock: false,
        alternate_titles: vec![SeriesAlternateTitleRecord {
            label: "alt".to_string(),
            title: "Alt Title".to_string(),
        }],
        alternate_titles_lock: false,
    }
}
