use std::fmt::{Display, Formatter};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use tantivy::collector::TopDocs;
use tantivy::doc;
use tantivy::query::{BooleanQuery, Occur, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TEXT, TantivyDocument, Value,
};
use tantivy::{Index, IndexReader, IndexWriter, ReloadPolicy, Term};

const LEGACY_LUCENE_ARTIFACT_PREFIXES: &[&str] = &["segments_", "write.lock", "segments.gen"];

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchDocument {
    pub entity_type: SearchEntityType,
    pub id: String,
    pub title: String,
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
    UnsafeLegacyIndexOwnership(PathBuf),
    CorruptedIndexRequiresExplicitRebuild(PathBuf, String),
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
            SearchError::UnsafeLegacyIndexOwnership(path) => write!(
                f,
                "legacy search directory '{}' is Java-owned; refusing non-destructive startup to avoid mixed-writer index wipe",
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

#[derive(Clone, Copy)]
struct SearchFields {
    doc_key: Field,
    entity_type: Field,
    entity_id: Field,
    title: Field,
}

impl SearchFields {
    fn from_schema(schema: &Schema) -> Result<Self, SearchError> {
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
        })
    }
}

pub fn startup_recover(index_dir: &Path) -> Result<(), SearchError> {
    fs::create_dir_all(index_dir)?;
    if looks_like_legacy_lucene_directory(index_dir)? {
        return Err(SearchError::UnsafeLegacyIndexOwnership(
            index_dir.to_path_buf(),
        ));
    }
    let _ = open_or_rebuild_index(index_dir, build_schema())?;
    Ok(())
}

impl SearchIndexLifecycle {
    pub fn bootstrap(index_dir: &Path) -> Result<Self, SearchError> {
        fs::create_dir_all(index_dir)?;

        let schema = build_schema();
        let index = open_or_rebuild_index(index_dir, schema.clone())?;
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
            add_doc(&mut writer, self.fields, document)?;
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
                add_doc(&mut writer, self.fields, &document)?;
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
        let parser = QueryParser::for_index(&self.index, vec![self.fields.title]);
        let parsed = parser
            .parse_query(query)
            .map_err(|error| SearchError::Query(error.to_string()))?;
        let type_query = TermQuery::new(
            Term::from_field_text(self.fields.entity_type, entity_type.as_str()),
            IndexRecordOption::Basic,
        );
        let query = BooleanQuery::new(vec![
            (Occur::Must, parsed),
            (Occur::Must, Box::new(type_query)),
        ]);

        let top_docs = searcher.search(&query, &TopDocs::with_limit(limit))?;
        let mut ids = Vec::with_capacity(top_docs.len());
        for (_score, address) in top_docs {
            let document: TantivyDocument = searcher.doc(address)?;
            let id: &str = document
                .get_first(self.fields.entity_id)
                .and_then(|value| value.as_str())
                .ok_or(SearchError::MissingStoredField("entity_id"))?;
            ids.push(id.to_string());
        }

        Ok(ids)
    }
}

fn build_schema() -> Schema {
    let mut schema = Schema::builder();
    schema.add_text_field("doc_key", STRING | STORED);
    schema.add_text_field("entity_type", STRING | STORED);
    schema.add_text_field("entity_id", STRING | STORED);
    schema.add_text_field("title", TEXT | STORED);
    schema.build()
}

fn add_doc(
    writer: &mut IndexWriter,
    fields: SearchFields,
    document: &SearchDocument,
) -> Result<(), SearchError> {
    let doc_key = document_key(document.entity_type, &document.id);
    writer.add_document(doc!(
        fields.doc_key => doc_key,
        fields.entity_type => document.entity_type.as_str(),
        fields.entity_id => document.id.clone(),
        fields.title => document.title.clone(),
    ))?;
    Ok(())
}

fn document_key(entity_type: SearchEntityType, id: &str) -> String {
    format!("{}:{id}", entity_type.as_str())
}

fn open_or_rebuild_index(index_dir: &Path, schema: Schema) -> Result<Index, SearchError> {
    let has_meta = index_dir.join("meta.json").exists();
    if has_meta {
        return Index::open_in_dir(index_dir).map_err(|error| {
            SearchError::CorruptedIndexRequiresExplicitRebuild(
                index_dir.to_path_buf(),
                error.to_string(),
            )
        });
    }

    Ok(Index::create_in_dir(index_dir, schema)?)
}

fn looks_like_legacy_lucene_directory(index_dir: &Path) -> Result<bool, SearchError> {
    let entries = fs::read_dir(index_dir)?;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if LEGACY_LUCENE_ARTIFACT_PREFIXES
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            return Ok(true);
        }
    }
    Ok(false)
}
