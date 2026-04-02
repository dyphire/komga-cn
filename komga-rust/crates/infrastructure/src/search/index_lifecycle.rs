use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::doc;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, Schema, TantivyDocument, Value, STORED, STRING, TEXT,
};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};

const LUCENE_ARTIFACT_PREFIXES: &[&str] = &["segments_", "write.lock", "segments.gen"];
const SUPPORTED_QUERY_FIELDS: &[&str] = &[
    "title",
    "isbn",
    "name",
    "publisher",
    "status",
    "reading_direction",
    "age_rating",
    "language",
    "genre",
    "sharing_label",
    "tag",
    "series_tag",
    "book_tag",
    "author",
    "writer",
    "penciller",
    "penciler",
    "inker",
    "colorist",
    "letterer",
    "cover",
    "editor",
    "translator",
    "release_date",
    "deleted",
    "oneshot",
    "complete",
    "total_book_count",
    "book_count",
];

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
        for field_name in SUPPORTED_QUERY_FIELDS {
            let field = schema
                .get_field(field_name)
                .map_err(|_| SearchError::MissingStoredField(field_name))?;
            query_fields.insert((*field_name).to_string(), field);
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
        Ok(index) => match SearchFields::from_schema(&index.schema()) {
            Ok(_) => Ok(SearchStartupLifecycle::Ready),
            Err(SearchError::MissingStoredField(_)) => Ok(SearchStartupLifecycle::RebuildRequired),
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
        let fields = SearchFields::from_schema(&schema)?;

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
        let mut parser = QueryParser::for_index(&self.index, default_fields);
        parser.set_conjunction_by_default();

        let parsed = match parser.parse_query(query) {
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
}

fn build_schema() -> Schema {
    let mut schema = Schema::builder();
    schema.add_text_field("doc_key", STRING | STORED);
    schema.add_text_field("entity_type", STRING | STORED);
    schema.add_text_field("entity_id", STRING | STORED);
    for field in SUPPORTED_QUERY_FIELDS {
        schema.add_text_field(field, TEXT | STORED);
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

        SearchFields::from_schema(&index.schema()).map_err(|error| {
            SearchError::CorruptedIndexRequiresExplicitRebuild(
                index_dir.to_path_buf(),
                format!("stale search schema/version detected: {error}"),
            )
        })?;

        return Ok(index);
    }

    Ok(Index::create_in_dir(index_dir, schema)?)
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        decide_startup_lifecycle, SearchDocument, SearchEntityType, SearchError, SearchFieldEntry,
        SearchIndexLifecycle, SearchStartupLifecycle,
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
}
