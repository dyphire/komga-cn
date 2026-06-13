use std::collections::BTreeSet;
#[cfg(unix)]
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::search::analyzer_profiles::query_tokenizer_profile_name;
use tantivy::schema::{FieldType, IndexRecordOption};

use super::{
    ANALYZER_VERSION_MARKER_FILE, SearchDocument, SearchEntityType, SearchError, SearchField,
    SearchFieldClass, SearchFieldEntry, SearchIndexLifecycle, SearchQueryLifecycle,
    SearchStartupLifecycle, build_query_tokenizer_manager, build_schema, decide_startup_lifecycle,
    index_tokenizer_profile_name, retained_query_fields, search_analyzer_version,
};

#[test]
fn bootstrap_rejects_lucene_artifacts() {
    let index_dir = temp_index_dir("bootstrap-rejects-lucene");
    std::fs::write(index_dir.join("segments_1"), b"owned").expect("write ownership marker");

    let result = SearchIndexLifecycle::bootstrap(index_dir.as_path());

    assert!(
        matches!(
            result,
            Err(SearchError::UnsafeLuceneIndexOwnership(path)) if path == index_dir
        ),
        "bootstrap must fail-closed when Lucene ownership artifacts are present",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn startup_lifecycle_marks_legacy_lucene_artifacts_rebuild_required() {
    let index_dir = temp_index_dir("startup-lifecycle-rebuilds-legacy-lucene");
    std::fs::write(index_dir.join("segments.gen"), b"owned").expect("write ownership marker");

    let result = decide_startup_lifecycle(index_dir.as_path());

    assert_eq!(
        result.expect("startup lifecycle should wipe-and-rebuild legacy Lucene state"),
        SearchStartupLifecycle::RebuildRequired,
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn bootstrap_refuses_corrupted_existing_meta_without_explicit_rebuild() {
    let index_dir = temp_index_dir("bootstrap-refuses-corrupted-meta");
    std::fs::write(index_dir.join("meta.json"), b"not-valid-json").expect("write corrupted meta");

    let result = SearchIndexLifecycle::bootstrap(index_dir.as_path());

    assert!(
        matches!(
            result,
            Err(SearchError::CorruptedIndexRequiresExplicitRebuild(path, _)) if path == index_dir
        ),
        "bootstrap must refuse destructive overwrite when existing index metadata is corrupted",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn startup_lifecycle_marks_existing_runtime_index_ready() {
    let index_dir = temp_index_dir("startup-lifecycle-existing-runtime-index");

    SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("bootstrap should create the runtime index fixture");

    let state = decide_startup_lifecycle(index_dir.as_path())
        .expect("startup lifecycle decision should inspect existing runtime index");

    assert_eq!(state, SearchStartupLifecycle::Ready);

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn startup_lifecycle_marks_stale_analyzer_version_rebuild_required() {
    let index_dir = temp_index_dir("startup-lifecycle-stale-analyzer-version");

    SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("bootstrap should create the runtime index fixture");
    std::fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        stale_analyzer_version().to_string(),
    )
    .expect("stale analyzer version marker should be writable");

    let state = decide_startup_lifecycle(index_dir.as_path())
        .expect("startup lifecycle should map stale analyzer version to rebuild required");

    assert_eq!(state, SearchStartupLifecycle::RebuildRequired);

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn bootstrap_opens_existing_runtime_index_without_rebuild() {
    let index_dir = temp_index_dir("bootstrap-opens-existing-runtime-index");

    let first = SearchIndexLifecycle::bootstrap(index_dir.as_path());
    assert!(first.is_ok(), "first bootstrap should create runtime index");
    drop(first);

    let second = SearchIndexLifecycle::bootstrap(index_dir.as_path());
    assert!(
        second.is_ok(),
        "second bootstrap should open existing runtime index without rebuild",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn query_bootstrap_succeeds_while_writer_lifecycle_is_alive() {
    let index_dir = temp_index_dir("query-bootstrap-with-live-writer");

    let writer_index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("writer lifecycle bootstrap should create runtime index");
    writer_index
        .rebuild(&[SearchDocument {
            entity_type: SearchEntityType::Collection,
            id: "collection-1".to_string(),
            title: "MIKI Shelf".to_string(),
            fields: vec![SearchFieldEntry::new(SearchField::Name, "MIKI Shelf")],
        }])
        .expect("writer lifecycle should index collection fixture");

    let query_index = SearchQueryLifecycle::bootstrap(index_dir.as_path())
        .expect("query lifecycle bootstrap should not compete for writer lock");
    let ids = query_index
        .search_ids("MIKI", SearchEntityType::Collection, 10)
        .expect("query lifecycle should search indexed collections");

    assert_eq!(ids, vec!["collection-1".to_string()]);

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn query_bootstrap_refuses_missing_index_without_creating_state() {
    let index_dir = temp_index_dir("query-bootstrap-refuses-missing-index");
    std::fs::remove_dir_all(&index_dir).expect("missing-index fixture should start absent");

    let result = SearchQueryLifecycle::bootstrap(index_dir.as_path());

    assert!(
        matches!(
            result,
            Err(SearchError::CorruptedIndexRequiresExplicitRebuild(path, _)) if path == index_dir
        ),
        "query lifecycle must fail-closed when the runtime index is absent instead of creating it on demand",
    );
    assert!(
        !index_dir.exists(),
        "query lifecycle must not create index directories while serving read-only searches",
    );
}

#[cfg(unix)]
#[test]
fn query_bootstrap_propagates_index_path_probe_errors() {
    let root = temp_index_dir("query-bootstrap-propagates-path-probe-errors");
    let file_component = root.join("not-a-directory");
    std::fs::write(&file_component, b"not a directory").expect("file component should be created");
    let index_dir = file_component.join("index");

    let result = SearchQueryLifecycle::bootstrap(index_dir.as_path());

    assert!(
        matches!(result, Err(SearchError::Io(error)) if error.kind() == ErrorKind::NotADirectory),
        "query lifecycle must not treat path probe errors as a missing index",
    );

    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bootstrap_refuses_existing_runtime_index_with_stale_analyzer_version() {
    let index_dir = temp_index_dir("bootstrap-refuses-stale-analyzer-version");

    SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("bootstrap should create the runtime index fixture");
    std::fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        stale_analyzer_version().to_string(),
    )
    .expect("stale analyzer version marker should be writable");

    let result = SearchIndexLifecycle::bootstrap(index_dir.as_path());

    assert!(
        matches!(
            result,
            Err(SearchError::CorruptedIndexRequiresExplicitRebuild(path, _)) if path == index_dir
        ),
        "bootstrap must fail-closed when existing analyzer version marker drifts",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_field_entries_use_typed_schema_contracts() {
    let entry = SearchFieldEntry::new(SearchField::Name, "Alpha Shelf");

    assert_eq!(entry.field, SearchField::Name);
    assert_eq!(entry.field.public_name(), "name");
    assert_eq!(entry.value, "Alpha Shelf");
}

#[test]
fn search_preserves_fielded_kotlin_visible_queries() {
    let index_dir = temp_index_dir("search-preserves-fielded-kotlin-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Collection,
                id: "collection-1".to_string(),
                title: "Alpha Shelf".to_string(),
                fields: vec![SearchFieldEntry::new(SearchField::Name, "Alpha Shelf")],
            },
            SearchDocument {
                entity_type: SearchEntityType::Collection,
                id: "collection-2".to_string(),
                title: "Beta Rack".to_string(),
                fields: vec![SearchFieldEntry::new(SearchField::Name, "Beta Rack")],
            },
        ])
        .expect("index rebuild should insert fixtures");

    let ids = index
        .search_ids("name:alpha", SearchEntityType::Collection, 10)
        .expect("fielded query should parse and execute");

    assert_eq!(
        ids,
        vec!["collection-1".to_string()],
        "kotlin-visible field names should remain usable in retained fielded queries",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn retained_query_field_contract_freezes_field_inventory_and_classes() {
    let expected = [
        ("title", SearchFieldClass::MultilingualFullText),
        ("isbn", SearchFieldClass::ExactTerm),
        ("name", SearchFieldClass::MultilingualFullText),
        ("publisher", SearchFieldClass::MultilingualFullText),
        ("status", SearchFieldClass::ExactTerm),
        ("reading_direction", SearchFieldClass::ExactTerm),
        ("age_rating", SearchFieldClass::ExactTerm),
        ("language", SearchFieldClass::ExactTerm),
        ("genre", SearchFieldClass::MultilingualFullText),
        ("sharing_label", SearchFieldClass::MultilingualFullText),
        ("tag", SearchFieldClass::MultilingualFullText),
        ("series_tag", SearchFieldClass::MultilingualFullText),
        ("book_tag", SearchFieldClass::MultilingualFullText),
        ("author", SearchFieldClass::MultilingualFullText),
        ("writer", SearchFieldClass::MultilingualFullText),
        ("penciller", SearchFieldClass::MultilingualFullText),
        ("penciler", SearchFieldClass::MultilingualFullText),
        ("inker", SearchFieldClass::MultilingualFullText),
        ("colorist", SearchFieldClass::MultilingualFullText),
        ("letterer", SearchFieldClass::MultilingualFullText),
        ("cover", SearchFieldClass::MultilingualFullText),
        ("editor", SearchFieldClass::MultilingualFullText),
        ("translator", SearchFieldClass::MultilingualFullText),
        ("release_date", SearchFieldClass::ExactTerm),
        ("deleted", SearchFieldClass::ExactTerm),
        ("oneshot", SearchFieldClass::ExactTerm),
        ("complete", SearchFieldClass::ExactTerm),
        ("total_book_count", SearchFieldClass::ExactTerm),
        ("book_count", SearchFieldClass::ExactTerm),
    ];

    let actual = retained_query_fields()
        .iter()
        .map(|field| (field.public_name(), field.class()))
        .collect::<Vec<_>>();

    assert_eq!(
        actual, expected,
        "search field inventory and analyzer class split are retained compatibility contracts",
    );
}

#[test]
fn retained_query_field_contract_has_no_duplicates_and_only_two_classes() {
    let fields = retained_query_fields();
    let unique_names = fields
        .iter()
        .map(|field| field.public_name())
        .collect::<BTreeSet<_>>();
    let unique_classes = fields
        .iter()
        .map(|field| field.class())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        unique_names.len(),
        fields.len(),
        "every retained public query field must be classified exactly once",
    );
    assert_eq!(
        unique_classes,
        BTreeSet::from([
            SearchFieldClass::MultilingualFullText,
            SearchFieldClass::ExactTerm,
        ]),
        "search analyzer compatibility should only expose the two retained field classes",
    );
}

#[test]
fn retained_query_fields_use_explicit_index_tokenizer_profiles() {
    let schema = build_schema();

    for search_field in retained_query_fields() {
        let schema_field = schema
            .get_field(search_field.public_name())
            .expect("retained query field should exist in schema");
        let tokenizer_name = match schema.get_field_entry(schema_field).field_type() {
            FieldType::Str(text_options) => text_options
                .get_indexing_options()
                .expect("retained query fields should stay indexed")
                .tokenizer(),
            other => panic!(
                "retained query field '{}' must remain text, got {:?}",
                search_field.public_name(),
                other
            ),
        };

        let expected = index_tokenizer_profile_name(search_field.class());

        assert_eq!(
            tokenizer_name,
            expected,
            "retained query field '{}' should use its explicit index analyzer profile",
            search_field.public_name(),
        );
    }
}

#[test]
fn retained_query_fields_bind_schema_index_options_by_analyzer_class() {
    let schema = build_schema();

    for search_field in retained_query_fields() {
        let schema_field = schema
            .get_field(search_field.public_name())
            .expect("retained query field should exist in schema");
        let index_option = match schema.get_field_entry(schema_field).field_type() {
            FieldType::Str(text_options) => text_options
                .get_indexing_options()
                .expect("retained query fields should stay indexed")
                .index_option(),
            other => panic!(
                "retained query field '{}' must remain text, got {:?}",
                search_field.public_name(),
                other
            ),
        };

        let expected = match search_field.class() {
            SearchFieldClass::MultilingualFullText => IndexRecordOption::WithFreqsAndPositions,
            SearchFieldClass::ExactTerm => IndexRecordOption::Basic,
        };

        assert_eq!(
            index_option,
            expected,
            "retained query field '{}' should bind schema index options through its analyzer class",
            search_field.public_name(),
        );
    }
}

#[test]
fn bootstrap_registers_explicit_index_and_query_tokenizer_profiles() {
    let index_dir = temp_index_dir("bootstrap-registers-explicit-tokenizer-profiles");
    let lifecycle = SearchIndexLifecycle::bootstrap(index_dir.as_path())
        .expect("index bootstrap should register tokenizer profiles");

    for tokenizer_name in [
        index_tokenizer_profile_name(SearchFieldClass::MultilingualFullText),
        query_tokenizer_profile_name(SearchFieldClass::MultilingualFullText),
        index_tokenizer_profile_name(SearchFieldClass::ExactTerm),
        query_tokenizer_profile_name(SearchFieldClass::ExactTerm),
    ] {
        assert!(
            lifecycle.index.tokenizers().get(&tokenizer_name).is_some(),
            "bootstrap should register tokenizer profile '{tokenizer_name}'",
        );
    }

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn retained_query_parser_uses_dedicated_query_side_tokenizer_manager() {
    let manager = build_query_tokenizer_manager();

    for class in [
        SearchFieldClass::MultilingualFullText,
        SearchFieldClass::ExactTerm,
    ] {
        assert!(
            manager.get(&index_tokenizer_profile_name(class)).is_some(),
            "query parser manager should expose index-bound tokenizer alias for {:?}",
            class,
        );
        assert!(
            manager.get(&query_tokenizer_profile_name(class)).is_none(),
            "query parser manager should stay dedicated to parser aliases instead of relying on raw query profile names for {:?}",
            class,
        );
    }
}

#[test]
fn exact_term_fields_do_not_match_partial_hyphenated_terms() {
    let index_dir = temp_index_dir("search-exact-term-fields-do-not-match-partials");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[SearchDocument {
            entity_type: SearchEntityType::Book,
            id: "book-1".to_string(),
            title: "One Shot".to_string(),
            fields: vec![
                SearchFieldEntry::new(SearchField::Isbn, "978-1-23"),
                SearchFieldEntry::new(SearchField::Status, "ONGOING"),
            ],
        }])
        .expect("index rebuild should insert exact-term fixture");

    let full_isbn_hits = index
        .search_ids("isbn:978-1-23", SearchEntityType::Book, 10)
        .expect("full exact isbn query should execute");
    let partial_isbn_hits = index
        .search_ids("isbn:978", SearchEntityType::Book, 10)
        .expect("partial exact isbn query should execute");
    let partial_status_hits = index
        .search_ids("status:ONGO", SearchEntityType::Book, 10)
        .expect("partial status query should execute");

    assert_eq!(
        full_isbn_hits,
        vec!["book-1".to_string()],
        "full exact isbn query should still match the retained field value",
    );
    assert!(
        partial_isbn_hits.is_empty(),
        "exact isbn fields must not match partial hyphen-split terms",
    );
    assert!(
        partial_status_hits.is_empty(),
        "exact status fields must not match partial prefixes",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_uses_default_and_semantics() {
    let index_dir = temp_index_dir("search-default-and-semantics");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "alpha beta".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "alpha only".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert fixtures");

    let ids = index
        .search_ids("alpha beta", SearchEntityType::Book, 10)
        .expect("default query should parse and execute");

    assert_eq!(
        ids,
        vec!["book-1".to_string()],
        "default query terms must use AND semantics to match Kotlin behavior",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_maps_parse_failure_to_empty_result_set() {
    let index_dir = temp_index_dir("search-parse-failure-empty-results");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[SearchDocument {
            entity_type: SearchEntityType::Book,
            id: "book-1".to_string(),
            title: "alpha".to_string(),
            fields: vec![],
        }])
        .expect("index rebuild should insert fixture");

    let ids = index
        .search_ids("title:(", SearchEntityType::Book, 10)
        .expect("invalid retained syntax should map to empty result set");

    assert!(
        ids.is_empty(),
        "invalid retained query syntax should return an empty candidate set",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_blank_input_returns_empty_result_set_without_error() {
    let index_dir = temp_index_dir("search-blank-input-empty-results");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[SearchDocument {
            entity_type: SearchEntityType::Book,
            id: "book-1".to_string(),
            title: "alpha".to_string(),
            fields: vec![],
        }])
        .expect("index rebuild should insert fixture");

    let ids = index
        .search_ids("   ", SearchEntityType::Book, 10)
        .expect("blank query should still execute");

    assert!(
        ids.is_empty(),
        "blank query input should remain an empty candidate set so route-level blank handling stays unchanged",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_preserves_fielded_role_queries() {
    let index_dir = temp_index_dir("search-preserves-fielded-role-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "Moon Hero".to_string(),
                fields: vec![SearchFieldEntry::new(SearchField::Writer, "Naoko Takeuchi")],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "Other Hero".to_string(),
                fields: vec![SearchFieldEntry::new(
                    SearchField::Writer,
                    "Rumiko Takahashi",
                )],
            },
        ])
        .expect("index rebuild should insert role fixtures");

    let ids = index
        .search_ids("writer:takeuchi", SearchEntityType::Book, 10)
        .expect("fielded role query should execute");

    assert_eq!(
        ids,
        vec!["book-1".to_string()],
        "retained role field names should keep parsing through the explicit query analyzer manager",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_multilingual_fields_match_accent_folded_queries() {
    let index_dir = temp_index_dir("search-multilingual-accent-folded-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "Café Society".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "Tea Plain".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert multilingual accent fixtures");

    let ids = index
        .search_ids("CAFE", SearchEntityType::Book, 10)
        .expect("accent-folded query should execute");

    assert_eq!(
        ids,
        vec!["book-1".to_string()],
        "multilingual fields should match accent-folded and lowercased queries against accented indexed values",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_preserves_mixed_latin_cjk_queries() {
    let index_dir = temp_index_dir("search-preserves-mixed-latin-cjk-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "Hero 東京".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "Hero Only".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert mixed-script fixtures");

    let ids = index
        .search_ids("hero 東京", SearchEntityType::Book, 10)
        .expect("mixed-script query should execute");

    assert_eq!(
        ids,
        vec!["book-1".to_string()],
        "mixed Latin and CJK tokens should keep parser behavior through explicit query analyzer wiring",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_multilingual_fields_match_mixed_width_queries() {
    let index_dir = temp_index_dir("search-multilingual-mixed-width-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "Ｈｅｒｏ　東京　１２３".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "Hero Only".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert mixed-width fixtures");

    let ids = index
        .search_ids("hero 東京 123", SearchEntityType::Book, 10)
        .expect("mixed-width query should execute");

    assert_eq!(
        ids,
        vec!["book-1".to_string()],
        "multilingual analyzers should normalize fullwidth latin and digits symmetrically across index and query paths",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_multilingual_fields_match_halfwidth_katakana_queries() {
    let index_dir = temp_index_dir("search-multilingual-halfwidth-katakana-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "ｶﾀｶﾅ Hero".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "Hero Only".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert halfwidth-katakana fixtures");

    let ids = index
        .search_ids("カタカナ hero", SearchEntityType::Book, 10)
        .expect("halfwidth-katakana query should execute");

    assert_eq!(
        ids,
        vec!["book-1".to_string()],
        "multilingual analyzers should normalize halfwidth katakana consistently across index and query paths",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_multilingual_fields_match_chinese_substring_queries() {
    let index_dir = temp_index_dir("search-multilingual-chinese-substring-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "不道德公會 河添太一 東立 搬运".to_string(),
                fields: vec![SearchFieldEntry::new(SearchField::Author, "河添太一")],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "正义联盟 英文版".to_string(),
                fields: vec![SearchFieldEntry::new(SearchField::Author, "Jane Writer")],
            },
        ])
        .expect("index rebuild should insert chinese substring fixtures");

    let title_ids = index
        .search_ids("公會", SearchEntityType::Book, 10)
        .expect("chinese substring query should execute");
    let mixed_title_ids = index
        .search_ids("title:添太", SearchEntityType::Book, 10)
        .expect("fielded chinese substring query should execute");
    let author_ids = index
        .search_ids("author:添太", SearchEntityType::Book, 10)
        .expect("author substring query should execute");

    assert_eq!(
        title_ids,
        vec!["book-1".to_string()],
        "multilingual title fields should converge on legacy-style CJK substring recall for Chinese queries",
    );
    assert_eq!(
        mixed_title_ids,
        vec!["book-1".to_string()],
        "fielded multilingual title queries should keep CJK substring recall without broadening exact fields",
    );
    assert_eq!(
        author_ids,
        vec!["book-1".to_string()],
        "multilingual author fields should pick up the same CJK substring recall approximation",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_multilingual_fields_match_hiragana_katakana_and_korean_substring_queries() {
    let index_dir = temp_index_dir("search-multilingual-cjk-substring-queries");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-hiragana".to_string(),
                title: "探偵はもう、死んでいる。".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-katakana".to_string(),
                title: "ワンパンマン Hero".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-korean".to_string(),
                title: "고교생을 환불해 주세요".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-mixed".to_string(),
                title: "Hero 不道德公會".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert cjk substring fixtures");

    let hiragana_ids = index
        .search_ids("んで", SearchEntityType::Book, 10)
        .expect("hiragana substring query should execute");
    let katakana_ids = index
        .search_ids("パン", SearchEntityType::Book, 10)
        .expect("katakana substring query should execute");
    let korean_ids = index
        .search_ids("환불", SearchEntityType::Book, 10)
        .expect("korean substring query should execute");
    let mixed_ids = index
        .search_ids("hero 公會", SearchEntityType::Book, 10)
        .expect("mixed-script substring query should execute");

    assert_eq!(
        hiragana_ids,
        vec!["book-hiragana".to_string()],
        "hiragana substring queries should retrieve the expected target document",
    );
    assert_eq!(
        katakana_ids,
        vec!["book-katakana".to_string()],
        "katakana substring queries should retrieve the expected target document",
    );
    assert_eq!(
        korean_ids,
        vec!["book-korean".to_string()],
        "korean substring queries should retrieve the expected target document",
    );
    assert_eq!(
        mixed_ids,
        vec!["book-mixed".to_string()],
        "mixed Latin+CJK queries should converge on the expected document set without requiring ranking identity",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_maps_punctuation_heavy_mixed_width_query_to_empty_result_set() {
    let index_dir = temp_index_dir("search-punctuation-heavy-mixed-width-empty-results");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[SearchDocument {
            entity_type: SearchEntityType::Book,
            id: "book-1".to_string(),
            title: "Hero 東京".to_string(),
            fields: vec![],
        }])
        .expect("index rebuild should insert punctuation-heavy parser fixture");

    let ids = index
        .search_ids("hero （東京", SearchEntityType::Book, 10)
        .expect("punctuation-heavy mixed-width query should map to empty result set");

    assert!(
        ids.is_empty(),
        "mixed-script queries that become malformed after width normalization should stay fail-closed instead of broad-matching surviving terms",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

#[test]
fn search_orders_equal_scores_by_id_for_determinism() {
    let index_dir = temp_index_dir("search-deterministic-id-tiebreak");
    let index =
        SearchIndexLifecycle::bootstrap(index_dir.as_path()).expect("index bootstrap should work");

    index
        .rebuild(&[
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-3".to_string(),
                title: "book".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "book".to_string(),
                fields: vec![],
            },
            SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-2".to_string(),
                title: "book".to_string(),
                fields: vec![],
            },
        ])
        .expect("index rebuild should insert equal-score fixtures");

    let ids = index
        .search_ids("book", SearchEntityType::Book, 10)
        .expect("search should return deterministic ids for equal scores");

    assert_eq!(
        ids,
        vec![
            "book-1".to_string(),
            "book-2".to_string(),
            "book-3".to_string()
        ],
        "equal-score retained results should use id ordering as deterministic tie-break",
    );

    let _ = std::fs::remove_dir_all(index_dir);
}

fn temp_index_dir(case: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "komga-rust-search-{case}-{}-{nanos}",
        std::process::id(),
    ));
    std::fs::create_dir_all(&dir).expect("temp index dir should be created");
    dir
}

fn stale_analyzer_version() -> u32 {
    search_analyzer_version().saturating_add(1)
}
