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

pub struct SearchQueryLifecycle {
    index: Index,
    reader: IndexReader,
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
    match prepare_index_directory(index_dir) {
        Ok(()) => {}
        Err(SearchError::UnsafeLuceneIndexOwnership(_)) => {
            // Startup is the one place where Rust is allowed to take over a legacy Kotlin Lucene
            // directory by wiping it first, because the server has already decided it owns writes.
            return Ok(SearchStartupLifecycle::RebuildRequired);
        }
        Err(error) => return Err(error),
    }

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
        let (index, reader, fields) = bootstrap_query_state(index_dir)?;
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

    pub fn rebuild_entities(
        &self,
        entity_types: &[SearchEntityType],
        docs: &[SearchDocument],
    ) -> Result<(), SearchError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|_| SearchError::WriterPoisoned)?;
        for entity_type in entity_types {
            writer.delete_term(Term::from_field_text(
                self.fields.entity_type,
                entity_type.as_str(),
            ));
        }
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

    pub fn search_scored_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, SearchError> {
        search_scored_ids(
            &self.index,
            &self.reader,
            &self.fields,
            query,
            entity_type,
            limit,
        )
    }

    pub fn search_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Result<Vec<String>, SearchError> {
        search_ids(
            &self.index,
            &self.reader,
            &self.fields,
            query,
            entity_type,
            limit,
        )
    }
}

impl SearchQueryLifecycle {
    pub fn bootstrap(index_dir: &Path) -> Result<Self, SearchError> {
        let (index, reader, fields) = bootstrap_existing_query_state(index_dir)?;

        Ok(Self {
            index,
            reader,
            fields,
        })
    }

    pub fn search_scored_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Result<Vec<(f32, String)>, SearchError> {
        search_scored_ids(
            &self.index,
            &self.reader,
            &self.fields,
            query,
            entity_type,
            limit,
        )
    }

    pub fn search_ids(
        &self,
        query: &str,
        entity_type: SearchEntityType,
        limit: usize,
    ) -> Result<Vec<String>, SearchError> {
        search_ids(
            &self.index,
            &self.reader,
            &self.fields,
            query,
            entity_type,
            limit,
        )
    }
}

fn bootstrap_query_state(
    index_dir: &Path,
) -> Result<(Index, IndexReader, SearchFields), SearchError> {
    prepare_index_directory(index_dir)?;

    let schema = build_schema();
    let index = open_or_create_index(index_dir, schema.clone())?;
    register_search_analyzer_profiles(&index);
    let fields = SearchFields::from_schema(&index.schema())?;

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    Ok((index, reader, fields))
}

fn bootstrap_existing_query_state(
    index_dir: &Path,
) -> Result<(Index, IndexReader, SearchFields), SearchError> {
    let index = open_existing_runtime_index(index_dir)?;
    register_search_analyzer_profiles(&index);
    let fields = SearchFields::from_schema(&index.schema())?;

    let reader = index
        .reader_builder()
        .reload_policy(ReloadPolicy::Manual)
        .try_into()?;
    Ok((index, reader, fields))
}

fn search_scored_ids(
    index: &Index,
    reader: &IndexReader,
    fields: &SearchFields,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Result<Vec<(f32, String)>, SearchError> {
    let searcher = reader.searcher();
    let parser = build_query_parser(index, fields, entity_type);
    let normalized_query = normalize_multilingual_width(query);
    let parsed = match parser.parse_query(normalized_query.as_ref()) {
        Ok(parsed) => parsed,
        Err(_) => return Ok(Vec::new()),
    };
    let type_query = TermQuery::new(
        Term::from_field_text(fields.entity_type, entity_type.as_str()),
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
            .get_first(fields.entity_id)
            .and_then(|value| value.as_str())
            .ok_or(SearchError::MissingStoredField("entity_id"))?;
        ranked_ids.push((score, id.to_string()));
    }

    ranked_ids.sort_by(|left, right| match right.0.total_cmp(&left.0) {
        std::cmp::Ordering::Equal => left.1.cmp(&right.1),
        ordering => ordering,
    });

    Ok(ranked_ids)
}

fn search_ids(
    index: &Index,
    reader: &IndexReader,
    fields: &SearchFields,
    query: &str,
    entity_type: SearchEntityType,
    limit: usize,
) -> Result<Vec<String>, SearchError> {
    search_scored_ids(index, reader, fields, query, entity_type, limit)
        .map(|ranked_ids| ranked_ids.into_iter().map(|(_, id)| id).collect())
}

fn build_query_parser(
    index: &Index,
    fields: &SearchFields,
    entity_type: SearchEntityType,
) -> QueryParser {
    let default_fields = entity_type
        .default_fields()
        .iter()
        .filter_map(|field_name| {
            fields
                .query_fields
                .get(translate_public_field_name(field_name))
                .copied()
        })
        .collect::<Vec<_>>();
    let mut parser = QueryParser::new(
        index.schema(),
        default_fields,
        build_query_tokenizer_manager(),
    );
    parser.set_conjunction_by_default();
    parser
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

fn open_existing_runtime_index(index_dir: &Path) -> Result<Index, SearchError> {
    if !index_dir.exists() || !index_dir.join("meta.json").exists() {
        return Err(SearchError::CorruptedIndexRequiresExplicitRebuild(
            index_dir.to_path_buf(),
            "search index does not exist yet".to_string(),
        ));
    }
    if has_lucene_artifacts(index_dir)? {
        return Err(SearchError::UnsafeLuceneIndexOwnership(
            index_dir.to_path_buf(),
        ));
    }

    let index = open_existing_index(index_dir)?;
    validate_existing_runtime_index(index_dir, &index).map_err(|error| {
        SearchError::CorruptedIndexRequiresExplicitRebuild(
            index_dir.to_path_buf(),
            format!("stale search schema/version detected: {error}"),
        )
    })?;
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
mod tests;
