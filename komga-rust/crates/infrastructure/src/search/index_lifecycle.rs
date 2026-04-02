use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use super::analyzer_profiles::{
    SearchFieldClass, build_query_time_analyzer, index_tokenizer_profile_name,
    normalize_multilingual_width, register_search_analyzer_profiles, search_analyzer_version,
    search_text_field_options,
};
use tantivy::collector::TopDocs;
use tantivy::doc;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, FieldType, IndexRecordOption, STORED, STRING, Schema, TantivyDocument, Value,
};
use tantivy::tokenizer::TokenizerManager;
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};

const LUCENE_ARTIFACT_PREFIXES: &[&str] = &["segments_", "write.lock", "segments.gen"];
const ANALYZER_VERSION_MARKER_FILE: &str = ".komga-search-analyzer-version";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct SearchFieldContract {
    public_name: &'static str,
    class: SearchFieldClass,
}

const RETAINED_QUERY_FIELD_CONTRACTS: &[SearchFieldContract] = &[
    SearchFieldContract {
        public_name: "title",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "isbn",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "name",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "publisher",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "status",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "reading_direction",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "age_rating",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "language",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "genre",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "sharing_label",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "tag",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "series_tag",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "book_tag",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "author",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "writer",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "penciller",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "penciler",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "inker",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "colorist",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "letterer",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "cover",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "editor",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "translator",
        class: SearchFieldClass::MultilingualFullText,
    },
    SearchFieldContract {
        public_name: "release_date",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "deleted",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "oneshot",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "complete",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "total_book_count",
        class: SearchFieldClass::ExactTerm,
    },
    SearchFieldContract {
        public_name: "book_count",
        class: SearchFieldClass::ExactTerm,
    },
];

fn retained_query_field_contracts() -> &'static [SearchFieldContract] {
    RETAINED_QUERY_FIELD_CONTRACTS
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchEntityType {
    Book,
    Series,
    Collection,
    ReadList,
}

impl SearchEntityType {
    fn as_str(&self) -> &'static str {
        match self {
            SearchEntityType::Book => "book",
            SearchEntityType::Series => "series",
            SearchEntityType::Collection => "collection",
            SearchEntityType::ReadList => "readlist",
        }
    }

    fn default_fields(&self) -> &'static [&'static str] {
        match self {
            SearchEntityType::Book => &["title", "isbn"],
            SearchEntityType::Series => &["title"],
            SearchEntityType::Collection => &["name"],
            SearchEntityType::ReadList => &["name"],
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchDocument {
    pub entity_type: SearchEntityType,
    pub id: String,
    pub title: String,
    pub fields: Vec<SearchFieldEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchFieldEntry {
    pub field: String,
    pub value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SearchEvent {
    Upsert(SearchDocument),
    Delete {
        entity_type: SearchEntityType,
        id: String,
    },
}

#[derive(Debug)]
pub enum SearchError {
    Io(std::io::Error),
    Tantivy(tantivy::TantivyError),
    Query(String),
    MissingStoredField(&'static str),
    UnexpectedTokenizerProfile {
        field: &'static str,
        expected: String,
        actual: String,
    },
    UnexpectedAnalyzerVersion {
        expected: u32,
        actual: Option<u32>,
    },
    WriterPoisoned,
    UnsafeLuceneIndexOwnership(PathBuf),
    CorruptedIndexRequiresExplicitRebuild(PathBuf, String),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SearchStartupLifecycle {
    Ready,
    RebuildRequired,
}

impl Display for SearchError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::Io(error) => write!(f, "search io error: {error}"),
            SearchError::Tantivy(error) => write!(f, "search tantivy error: {error}"),
            SearchError::Query(error) => write!(f, "search query parse error: {error}"),
            SearchError::MissingStoredField(field) => {
                write!(f, "search stored field missing: {field}")
            }
            SearchError::UnexpectedTokenizerProfile {
                field,
                expected,
                actual,
            } => write!(
                f,
                "search field '{field}' uses tokenizer profile '{actual}' but expected '{expected}'"
            ),
            SearchError::UnexpectedAnalyzerVersion { expected, actual } => match actual {
                Some(actual) => write!(
                    f,
                    "search index analyzer version '{actual}' does not match expected '{expected}'"
                ),
                None => write!(
                    f,
                    "search index analyzer version marker is missing; expected '{expected}'"
                ),
            },
            SearchError::WriterPoisoned => write!(f, "search index writer lock poisoned"),
            SearchError::UnsafeLuceneIndexOwnership(path) => write!(
                f,
                "lucene search directory '{}' is external-owned; refusing non-destructive startup to avoid mixed-writer index wipe",
                path.display(),
            ),
            SearchError::CorruptedIndexRequiresExplicitRebuild(path, source) => write!(
                f,
                "search index startup refused to overwrite existing state at '{}'; explicit rebuild is required ({source})",
                path.display(),
            ),
        }
    }
}

impl std::error::Error for SearchError {}

impl From<std::io::Error> for SearchError {
    fn from(value: std::io::Error) -> Self {
        SearchError::Io(value)
    }
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(value: tantivy::TantivyError) -> Self {
        SearchError::Tantivy(value)
    }
}

pub struct SearchIndexLifecycle {
    index: Index,
    reader: IndexReader,
    writer: Mutex<IndexWriter>,
    fields: SearchFields,
}

#[derive(Clone)]
struct SearchFields {
    doc_key: Field,
    entity_type: Field,
    entity_id: Field,
    title: Field,
    query_fields: BTreeMap<String, Field>,
}

impl SearchFields {
    fn from_schema(schema: &Schema) -> Result<Self, SearchError> {
        let mut query_fields = BTreeMap::new();
        for contract in retained_query_field_contracts() {
            let schema_field = schema
                .get_field(contract.public_name)
                .map_err(|_| SearchError::MissingStoredField(contract.public_name))?;
            let tokenizer_name = match schema.get_field_entry(schema_field).field_type() {
                FieldType::Str(text_options) => text_options
                    .get_indexing_options()
                    .map(|indexing_options| indexing_options.tokenizer())
                    .ok_or(SearchError::MissingStoredField(contract.public_name))?,
                _ => return Err(SearchError::MissingStoredField(contract.public_name)),
            };
            let expected_tokenizer = index_tokenizer_profile_name(contract.class);
            if tokenizer_name != expected_tokenizer {
                return Err(SearchError::UnexpectedTokenizerProfile {
                    field: contract.public_name,
                    expected: expected_tokenizer,
                    actual: tokenizer_name.to_string(),
                });
            }
            query_fields.insert(contract.public_name.to_string(), schema_field);
        }

        Ok(Self {
            doc_key: schema
                .get_field("doc_key")
                .map_err(|_| SearchError::MissingStoredField("doc_key"))?,
            entity_type: schema
                .get_field("entity_type")
                .map_err(|_| SearchError::MissingStoredField("entity_type"))?,
            entity_id: schema
                .get_field("entity_id")
                .map_err(|_| SearchError::MissingStoredField("entity_id"))?,
            title: schema
                .get_field("title")
                .map_err(|_| SearchError::MissingStoredField("title"))?,
            query_fields,
        })
    }
}

pub fn decide_startup_lifecycle(index_dir: &Path) -> Result<SearchStartupLifecycle, SearchError> {
    prepare_index_directory(index_dir)?;

    if !index_dir.join("meta.json").exists() {
        return Ok(SearchStartupLifecycle::RebuildRequired);
    }

    match open_existing_index(index_dir) {
        Ok(index) => match validate_existing_runtime_index(index_dir, &index) {
            Ok(_) => Ok(SearchStartupLifecycle::Ready),
            Err(
                SearchError::MissingStoredField(_)
                | SearchError::UnexpectedTokenizerProfile { .. }
                | SearchError::UnexpectedAnalyzerVersion { .. },
            ) => Ok(SearchStartupLifecycle::RebuildRequired),
            Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
                Ok(SearchStartupLifecycle::RebuildRequired)
            }
            Err(error) => Err(error),
        },
        Err(SearchError::CorruptedIndexRequiresExplicitRebuild(_, _)) => {
            Ok(SearchStartupLifecycle::RebuildRequired)
        }
        Err(error) => Err(error),
    }
}

pub fn prepare_for_rebuild(index_dir: &Path) -> Result<(), SearchError> {
    if index_dir.exists() {
        fs::remove_dir_all(index_dir)?;
    }
    fs::create_dir_all(index_dir)?;
    Ok(())
}

impl SearchIndexLifecycle {
    pub fn bootstrap(index_dir: &Path) -> Result<Self, SearchError> {
        prepare_index_directory(index_dir)?;

        let schema = build_schema();
        let index = open_or_create_index(index_dir, schema.clone())?;
        register_search_analyzer_profiles(&index);
        let fields = SearchFields::from_schema(&index.schema())?;

        let reader = index
            .reader_builder()
            .reload_policy(ReloadPolicy::Manual)
            .try_into()?;
        let writer = index.writer(50_000_000)?;

        Ok(Self {
            index,
            reader,
            writer: Mutex::new(writer),
            fields,
        })
    }

    pub fn rebuild(&self, docs: &[SearchDocument]) -> Result<(), SearchError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| SearchError::WriterPoisoned)?;
        writer.delete_all_documents()?;
        for document in docs {
            add_doc(&mut writer, &self.fields, document)?;
        }
        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn apply_event(&self, event: SearchEvent) -> Result<(), SearchError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| SearchError::WriterPoisoned)?;

        match event {
            SearchEvent::Upsert(document) => {
                let key = document_key(document.entity_type, &document.id);
                writer.delete_term(Term::from_field_text(self.fields.doc_key, &key));
                add_doc(&mut writer, &self.fields, &document)?;
            }
            SearchEvent::Delete { entity_type, id } => {
                let key = document_key(entity_type, &id);
                writer.delete_term(Term::from_field_text(self.fields.doc_key, &key));
            }
        }

        writer.commit()?;
        self.reader.reload()?;
        Ok(())
    }

    pub fn search_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Result<Vec<String>, SearchError> {
        let searcher = self.reader.searcher();
        let parser = self.build_query_parser(entity_type);
        let normalized_query = normalize_multilingual_width(query);

        let parsed = match parser.parse_query(normalized_query.as_ref()) {
            Ok(parsed) => parsed,
            Err(_) => return Ok(Vec::new()),
        };
        let type_query = TermQuery::new(
            Term::from_field_text(self.fields.entity_type, entity_type.as_str()),
            IndexRecordOption::Basic,
        );
        let query = BooleanQuery::new(vec![
            (Occur::Must, parsed),
            (Occur::Must, Box::new(type_query)),
        ]);

        let mut ranked_ids = Vec::new();
        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit).order_by_score())?;
        for (score, address) in top_docs {
            let document: TantivyDocument = searcher.doc(address)?;
            let id: &str = document
                .get_first(self.fields.entity_id)
                .and_then(|value| value.as_str())
                .ok_or(SearchError::MissingStoredField("entity_id"))?;
            ranked_ids.push((score, id.to_string()));
        }

        ranked_ids.sort_by(|left, right| match right.0.total_cmp(&left.0) {
            std::cmp::Ordering::Equal => left.1.cmp(&right.1),
            ordering => ordering,
        });

        Ok(ranked_ids.into_iter().map(|(_, id)| id).collect())
    }

    fn build_query_parser(&self, entity_type: SearchEntityType) -> QueryParser {
        let default_fields = entity_type
            .default_fields()
            .iter()
            .filter_map(|field_name| {
                self.fields
                    .query_fields
                    .get(translate_public_field_name(field_name))
                    .copied()
            })
            .collect::<Vec<_>>();
        let mut parser = QueryParser::new(
            self.index.schema(),
            default_fields,
            build_query_tokenizer_manager(),
        );
        parser.set_conjunction_by_default();
        parser
    }
}

fn build_query_tokenizer_manager() -> TokenizerManager {
    let manager = TokenizerManager::default();
    for class in [
        SearchFieldClass::MultilingualFullText,
        SearchFieldClass::ExactTerm,
    ] {
        manager.register(
            &index_tokenizer_profile_name(class),
            build_query_time_analyzer(class),
        );
    }
    manager
}

fn build_schema() -> Schema {
    let mut schema = Schema::builder();
    schema.add_text_field("doc_key", STRING | STORED);
    schema.add_text_field("entity_type", STRING | STORED);
    schema.add_text_field("entity_id", STRING | STORED);
    for contract in retained_query_field_contracts() {
        schema.add_text_field(
            contract.public_name,
            search_text_field_options(contract.class),
        );
    }
    schema.build()
}

fn add_doc(
    writer: &mut IndexWriter,
    fields: &SearchFields,
    document: &SearchDocument,
) -> Result<(), SearchError> {
    let doc_key = document_key(document.entity_type, &document.id);
    let mut tantivy_document = doc!(
        fields.doc_key => doc_key,
        fields.entity_type => document.entity_type.as_str(),
        fields.entity_id => document.id.clone(),
        fields.title => document.title.clone(),
    );

    for extra in &document.fields {
        let field_name = translate_public_field_name(&extra.field);
        if field_name == "title" {
            tantivy_document.add_text(fields.title, extra.value.clone());
            continue;
        }
        if let Some(field) = fields.query_fields.get(field_name) {
            tantivy_document.add_text(*field, extra.value.clone());
        }
    }

    writer.add_document(tantivy_document)?;
    Ok(())
}

fn translate_public_field_name(field_name: &str) -> &str {
    field_name
}

fn document_key(entity_type: SearchEntityType, id: &str) -> String {
    format!("{}:{id}", entity_type.as_str())
}

fn open_or_create_index(index_dir: &Path, schema: Schema) -> Result<Index, SearchError> {
    if index_dir.join("meta.json").exists() {
        let index = open_existing_index(index_dir)?;

        validate_existing_runtime_index(index_dir, &index).map_err(|error| {
            SearchError::CorruptedIndexRequiresExplicitRebuild(
                index_dir.to_path_buf(),
                format!("stale search schema/version detected: {error}"),
            )
        })?;

        return Ok(index);
    }

    let index = Index::create_in_dir(index_dir, schema)?;
    write_current_analyzer_version_marker(index_dir)?;
    Ok(index)
}

fn open_existing_index(index_dir: &Path) -> Result<Index, SearchError> {
    Index::open_in_dir(index_dir).map_err(|error| {
        SearchError::CorruptedIndexRequiresExplicitRebuild(
            index_dir.to_path_buf(),
            error.to_string(),
        )
    })
}

fn prepare_index_directory(index_dir: &Path) -> Result<(), SearchError> {
    fs::create_dir_all(index_dir)?;
    if has_lucene_artifacts(index_dir)? {
        return Err(SearchError::UnsafeLuceneIndexOwnership(
            index_dir.to_path_buf(),
        ));
    }
    Ok(())
}

fn has_lucene_artifacts(index_dir: &Path) -> Result<bool, SearchError> {
    let entries = fs::read_dir(index_dir)?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if LUCENE_ARTIFACT_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

fn validate_existing_runtime_index(index_dir: &Path, index: &Index) -> Result<(), SearchError> {
    SearchFields::from_schema(&index.schema())?;
    validate_analyzer_version_marker(index_dir)
}

fn validate_analyzer_version_marker(index_dir: &Path) -> Result<(), SearchError> {
    let expected = search_analyzer_version();
    let marker_path = index_dir.join(ANALYZER_VERSION_MARKER_FILE);
    let raw = match fs::read_to_string(&marker_path) {
        Ok(raw) => raw,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(SearchError::UnexpectedAnalyzerVersion {
                expected,
                actual: None,
            });
        }
        Err(error) => {
            return Err(SearchError::CorruptedIndexRequiresExplicitRebuild(
                index_dir.to_path_buf(),
                format!(
                    "failed to read analyzer version marker '{}': {error}",
                    marker_path.display()
                ),
            ));
        }
    };

    let actual = raw.trim().parse::<u32>().map_err(|error| {
        SearchError::CorruptedIndexRequiresExplicitRebuild(
            index_dir.to_path_buf(),
            format!(
                "invalid analyzer version marker '{}': {error}",
                marker_path.display()
            ),
        )
    })?;

    if actual == expected {
        Ok(())
    } else {
        Err(SearchError::UnexpectedAnalyzerVersion {
            expected,
            actual: Some(actual),
        })
    }
}

fn write_current_analyzer_version_marker(index_dir: &Path) -> Result<(), SearchError> {
    fs::write(
        index_dir.join(ANALYZER_VERSION_MARKER_FILE),
        search_analyzer_version().to_string(),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::search::analyzer_profiles::query_tokenizer_profile_name;
    use tantivy::schema::{FieldType, IndexRecordOption};

    use super::{
        ANALYZER_VERSION_MARKER_FILE, SearchDocument, SearchEntityType, SearchError,
        SearchFieldClass, SearchFieldEntry, SearchIndexLifecycle, SearchStartupLifecycle,
        build_query_tokenizer_manager, build_schema, decide_startup_lifecycle,
        index_tokenizer_profile_name, retained_query_field_contracts, search_analyzer_version,
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
    fn startup_lifecycle_rejects_lucene_artifacts() {
        let index_dir = temp_index_dir("startup-lifecycle-rejects-lucene");
        std::fs::write(index_dir.join("segments.gen"), b"owned").expect("write ownership marker");

        let result = decide_startup_lifecycle(index_dir.as_path());

        assert!(
            matches!(
                result,
                Err(SearchError::UnsafeLuceneIndexOwnership(path)) if path == index_dir
            ),
            "startup lifecycle must fail-closed when Lucene ownership artifacts are present",
        );

        let _ = std::fs::remove_dir_all(index_dir);
    }

    #[test]
    fn bootstrap_refuses_corrupted_existing_meta_without_explicit_rebuild() {
        let index_dir = temp_index_dir("bootstrap-refuses-corrupted-meta");
        std::fs::write(index_dir.join("meta.json"), b"not-valid-json")
            .expect("write corrupted meta");

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
    fn search_preserves_fielded_kotlin_visible_queries() {
        let index_dir = temp_index_dir("search-preserves-fielded-kotlin-queries");
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

        index
            .rebuild(&[
                SearchDocument {
                    entity_type: SearchEntityType::Collection,
                    id: "collection-1".to_string(),
                    title: "Alpha Shelf".to_string(),
                    fields: vec![SearchFieldEntry {
                        field: "name".to_string(),
                        value: "Alpha Shelf".to_string(),
                    }],
                },
                SearchDocument {
                    entity_type: SearchEntityType::Collection,
                    id: "collection-2".to_string(),
                    title: "Beta Rack".to_string(),
                    fields: vec![SearchFieldEntry {
                        field: "name".to_string(),
                        value: "Beta Rack".to_string(),
                    }],
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

        let actual = retained_query_field_contracts()
            .iter()
            .map(|field| (field.public_name, field.class))
            .collect::<Vec<_>>();

        assert_eq!(
            actual, expected,
            "search field inventory and analyzer class split are retained compatibility contracts",
        );
    }

    #[test]
    fn retained_query_field_contract_has_no_duplicates_and_only_two_classes() {
        let contracts = retained_query_field_contracts();
        let unique_names = contracts
            .iter()
            .map(|field| field.public_name)
            .collect::<BTreeSet<_>>();
        let unique_classes = contracts
            .iter()
            .map(|field| field.class)
            .collect::<BTreeSet<_>>();

        assert_eq!(
            unique_names.len(),
            contracts.len(),
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

        for contract in retained_query_field_contracts() {
            let field = schema
                .get_field(contract.public_name)
                .expect("retained query field should exist in schema");
            let tokenizer_name = match schema.get_field_entry(field).field_type() {
                FieldType::Str(text_options) => text_options
                    .get_indexing_options()
                    .expect("retained query fields should stay indexed")
                    .tokenizer(),
                other => panic!(
                    "retained query field '{}' must remain text, got {:?}",
                    contract.public_name, other
                ),
            };

            let expected = index_tokenizer_profile_name(contract.class);

            assert_eq!(
                tokenizer_name, expected,
                "retained query field '{}' should use its explicit index analyzer profile",
                contract.public_name,
            );
        }
    }

    #[test]
    fn retained_query_fields_bind_schema_index_options_by_analyzer_class() {
        let schema = build_schema();

        for contract in retained_query_field_contracts() {
            let field = schema
                .get_field(contract.public_name)
                .expect("retained query field should exist in schema");
            let index_option = match schema.get_field_entry(field).field_type() {
                FieldType::Str(text_options) => text_options
                    .get_indexing_options()
                    .expect("retained query fields should stay indexed")
                    .index_option(),
                other => panic!(
                    "retained query field '{}' must remain text, got {:?}",
                    contract.public_name, other
                ),
            };

            let expected = match contract.class {
                SearchFieldClass::MultilingualFullText => IndexRecordOption::WithFreqsAndPositions,
                SearchFieldClass::ExactTerm => IndexRecordOption::Basic,
            };

            assert_eq!(
                index_option, expected,
                "retained query field '{}' should bind schema index options through its analyzer class",
                contract.public_name,
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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

        index
            .rebuild(&[SearchDocument {
                entity_type: SearchEntityType::Book,
                id: "book-1".to_string(),
                title: "One Shot".to_string(),
                fields: vec![
                    SearchFieldEntry {
                        field: "isbn".to_string(),
                        value: "978-1-23".to_string(),
                    },
                    SearchFieldEntry {
                        field: "status".to_string(),
                        value: "ONGOING".to_string(),
                    },
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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

        index
            .rebuild(&[
                SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: "book-1".to_string(),
                    title: "Moon Hero".to_string(),
                    fields: vec![SearchFieldEntry {
                        field: "writer".to_string(),
                        value: "Naoko Takeuchi".to_string(),
                    }],
                },
                SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: "book-2".to_string(),
                    title: "Other Hero".to_string(),
                    fields: vec![SearchFieldEntry {
                        field: "writer".to_string(),
                        value: "Rumiko Takahashi".to_string(),
                    }],
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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

        index
            .rebuild(&[
                SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: "book-1".to_string(),
                    title: "不道德公會 河添太一 東立 搬运".to_string(),
                    fields: vec![SearchFieldEntry {
                        field: "author".to_string(),
                        value: "河添太一".to_string(),
                    }],
                },
                SearchDocument {
                    entity_type: SearchEntityType::Book,
                    id: "book-2".to_string(),
                    title: "正义联盟 英文版".to_string(),
                    fields: vec![SearchFieldEntry {
                        field: "author".to_string(),
                        value: "Jane Writer".to_string(),
                    }],
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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
        let index = SearchIndexLifecycle::bootstrap(index_dir.as_path())
            .expect("index bootstrap should work");

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
}
